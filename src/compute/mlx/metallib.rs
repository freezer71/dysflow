//! Les noyaux Metal de la copie Rust de MLX. Laissé à lui-même, MLX
//! chercherait aussi à un chemin de la machine de build et, sans noyaux,
//! `mlx-rs` paniquerait : on désigne le fichier et on le charge avant tout
//! autre appel.
//!
//! Le fichier est `mlx.metallib` à côté de l'exécutable.
//! `DYSFLOW_MLX_METALLIB` (un fichier) prime, pour les exemples et les tests
//! de `ffi/` ; les tests de ce paquet acceptent aussi `target/mlx-metallib`,
//! où scripts/mlx-sources.sh le fait écrire.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

const FILE: &str = "mlx.metallib";
const VARIABLE: &str = "DYSFLOW_MLX_METALLIB";

/// `<exe_dir>/mlx.metallib`, ou une erreur qui le nomme.
pub fn locate_from(exe_dir: &Path) -> Result<PathBuf> {
    existing(exe_dir.join(FILE))
}

fn existing(path: PathBuf) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path);
    }
    Err(anyhow!(crate::tr!(
        en = "MLX Metal kernels not found: {}",
        fr = "noyaux Metal de MLX introuvables : {}",
        path.display(),
    )))
}

/// `variable` prime, sans repli ; vide, elle ne compte pas.
fn locate_with(variable: Option<OsString>, exe_dir: &Path) -> Result<PathBuf> {
    match variable.filter(|value| !value.is_empty()) {
        Some(file) => existing(file.into()),
        None => locate_from(exe_dir),
    }
}

fn locate() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .context(crate::tr!("cannot locate the executable", "exécutable introuvable"))?;
    // Un binaire lancé par un lien : ses noyaux sont à côté de sa cible.
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe.parent().unwrap_or(Path::new("")).to_path_buf();
    #[cfg(test)]
    let dir = if dir.join(FILE).is_file() {
        dir
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mlx-metallib")
    };
    locate_with(std::env::var_os(VARIABLE), &dir)
}

/// Sur le fil MLX, avant son premier appel MLX : désigne les noyaux et les
/// charge, pour qu'un fichier illisible soit une erreur ici et non une
/// panique au premier calcul (`Stream::gpu` de mlx-rs ne rend pas d'erreur).
pub(super) fn install() -> Result<()> {
    let path = locate()?;
    let unusable = |why: &str| {
        anyhow!(crate::tr!(
            en = "MLX cannot use the Metal kernels {}: {}",
            fr = "MLX ne peut pas utiliser les noyaux Metal {} : {}",
            path.display(),
            why,
        ))
    };
    let name = path.to_str().ok_or_else(|| unusable(crate::tr!("path is not UTF-8", "chemin non UTF-8")))?;
    mlx_rs::metal::set_metallib_path(name).map_err(|err| unusable(err.what()))?;
    // Le premier flux du GPU ouvre le périphérique, qui lit le fichier.
    mlx_rs::Stream::try_default_on_device(&mlx_rs::Device::gpu()).map_err(|err| unusable(err.what()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier neuf, retiré en fin de test.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("dysflow-metallib-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_metallib_is_an_error_that_names_the_path() {
        let dir = Dir::new("absent");
        let err = locate_from(dir.path()).unwrap_err().to_string();
        assert!(err.contains("mlx.metallib"));
        assert!(err.contains(&dir.path().display().to_string()), "{err}");
        let english = || locate_from(dir.path()).unwrap_err().to_string();
        let en = crate::i18n::with_ui_lang(crate::i18n::UiLang::En, english);
        assert!(en.contains("not found"), "{en}");
        assert!(err.contains("introuvable"), "{err}");
    }

    #[test]
    fn the_metallib_next_to_the_executable_is_found() {
        let dir = Dir::new("present");
        let file = dir.path().join(FILE);
        std::fs::write(&file, b"noyaux").unwrap();
        assert_eq!(locate_from(dir.path()).unwrap(), file);
    }

    #[test]
    fn the_variable_wins_and_never_falls_back() {
        let dir = Dir::new("variable");
        let beside = dir.path().join(FILE);
        let chosen = dir.path().join("choisie.metallib");
        std::fs::write(&beside, b"noyaux").unwrap();
        std::fs::write(&chosen, b"noyaux").unwrap();
        assert_eq!(locate_with(Some(chosen.clone().into()), dir.path()).unwrap(), chosen);
        assert_eq!(locate_with(Some("".into()), dir.path()).unwrap(), beside);
        assert_eq!(locate_with(None, dir.path()).unwrap(), beside);
        let gone = dir.path().join("partie.metallib");
        let err = locate_with(Some(gone.into()), dir.path()).unwrap_err().to_string();
        assert!(err.contains("partie.metallib"), "{err}");
    }
}

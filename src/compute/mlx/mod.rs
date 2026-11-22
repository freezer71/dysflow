//! MLX en Rust (`mlx-rs`), sur les Mac Apple Silicon : un seul fil (`Held`),
//! sous le verrou commun avec mlx-swift (`compute::gpu`), avec ses plafonds
//! (`budget`) et ses noyaux Metal (`metallib`).

pub mod budget;
pub mod metallib;
mod worker;

pub use worker::Held;
#[cfg(test)]
pub(crate) use worker::queued;

/// Un calcul MLX minimal : la somme de `[1, 2, 3]` sur le GPU, 6. Prouve que
/// MLX, compilé par `mlx-rs`, tourne dans le processus (le terminal, ou l'app
/// à côté de mlx-swift).
pub fn smoke() -> anyhow::Result<f32> {
    worker::run(|| {
        mlx_rs::with_stream(&mlx_rs::Stream::gpu(), || {
            let sum = mlx_rs::Array::from_slice(&[1.0f32, 2.0, 3.0], &[3]).sum(None)?;
            sum.eval()?;
            Ok(sum.try_item_exact::<f32>()?)
        })
    })
}

/// La mémoire de la copie Rust de MLX, en octets : en usage, au plus haut
/// depuis le dernier `reset_peak`, et en cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub active: usize,
    pub peak: usize,
    pub cache: usize,
}

/// Lue sur le fil MLX, entre deux travaux (les mesures, `voice_bench`).
pub fn memory() -> anyhow::Result<Memory> {
    worker::run(|| {
        Ok(Memory {
            active: mlx_rs::memory::active_memory()?,
            peak: mlx_rs::memory::peak_memory()?,
            cache: mlx_rs::memory::cache_memory()?,
        })
    })
}

/// Repart de la mémoire en usage pour le pic suivant.
pub fn reset_peak() -> anyhow::Result<()> {
    worker::run(|| Ok(mlx_rs::memory::reset_peak_memory()?))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn the_memory_report_is_read_on_the_worker() {
        let memory = super::memory().expect("MLX");
        assert!(memory.peak >= memory.active, "{memory:?}");
    }

    #[test]
    fn mlx_sums_on_the_gpu() {
        assert_eq!(super::smoke().expect("MLX"), 6.0);
    }

    /// Les noyaux sont ceux du MLX lié : ceux d'une autre version de macOS
    /// n'ont pas les mêmes fonctions (NAX), et la somme ci-dessus ne le voit
    /// pas. Un produit de matrices, si.
    #[test]
    fn mlx_multiplies_with_the_kernels_it_was_built_with() {
        let total = super::worker::run(|| {
            let ones = mlx_rs::ops::ones::<f32>(&[512, 512])?;
            let product = ones.matmul(&ones)?.sum(None)?;
            product.eval()?;
            Ok(product.try_item_exact::<f32>()?)
        });
        assert_eq!(total.expect("MLX"), (512 * 512 * 512) as f32);
    }

    /// Sans noyaux, ou avec des noyaux illisibles : une erreur qui nomme le
    /// fichier, jamais une panique ni un arrêt. Dans un processus à part, MLX
    /// n'y est pas encore prêt.
    #[test]
    fn no_kernels_is_an_error_that_names_the_file() {
        const CHILD: &str = "DYSFLOW_TEST_WITHOUT_KERNELS";
        if let Some(path) = std::env::var_os(CHILD) {
            let path = path.to_string_lossy();
            super::budget::set("test:sans-noyaux", super::budget::Need { memory: 1 << 30, cache: 1 << 20 });
            for err in [super::smoke().map(drop).unwrap_err(), super::Held::load(|| Ok(1)).map(drop).unwrap_err()] {
                let err = format!("{err:#}");
                assert!(err.contains(&*path), "{err}");
                assert!(!err.contains("a paniqué"), "{err}");
            }
            return;
        }
        let dir = std::env::temp_dir().join(format!("dysflow-noyaux-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let unreadable = dir.join("illisible.metallib");
        std::fs::write(&unreadable, b"pas des noyaux").unwrap();
        for path in [dir.join("absente.metallib"), unreadable] {
            let out = Command::new(std::env::current_exe().unwrap())
                .args(["compute::mlx::tests::no_kernels_is_an_error_that_names_the_file", "--exact"])
                .env(CHILD, &path)
                .env("DYSFLOW_MLX_METALLIB", &path)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success() && stdout.contains("1 passed"), "{stdout}{stderr}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

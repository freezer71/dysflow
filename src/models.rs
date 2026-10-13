//! Catalogue, installation et suppression des modèles TTS.
//!
//! Sans modèle, le lecteur se replie sur la voix du système — utilisable, mais
//! ce n'est pas ce qu'on attend d'un lecteur. Ce module rend l'installation
//! d'un vrai modèle atteignable depuis l'interface, sans ligne de commande et
//! sans jamais télécharger quoi que ce soit sans accord explicite.

use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Un fichier de plus, téléchargé dans le dossier de la famille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Companion {
    pub repo: &'static str,
    pub revision: &'static str,
    /// Chemin dans le dépôt.
    pub file: &'static str,
    /// Chemin sous `models/<famille>/`. Deux paquets d'une famille qui
    /// nomment le même `dest` le partagent.
    pub dest: &'static str,
    pub bytes: u64,
}

impl Companion {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.revision, self.file)
    }
}

/// Ce qu'un paquet demande pour se charger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// Une famille compilée dans audio.cpp.
    AudioCpp,
    /// Kokoro en MLX : seulement dans l'app, qui fournit le calcul.
    KokoroMlx,
    /// Kokoro par ONNX Runtime : seulement avec la feature `kokoro-onnx`.
    KokoroOnnx,
}

/// Un paquet téléchargeable : une variante de précision d'une famille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPackage {
    pub id: &'static str,
    pub family: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    /// Précision des poids, qui arbitre taille contre qualité.
    pub precision: &'static str,
    pub languages: &'static [&'static str],
    pub repo: &'static str,
    pub revision: &'static str,
    /// Chemin du fichier dans le dépôt.
    pub file: &'static str,
    /// Comment annoncer la couverture linguistique.
    ///
    /// Certains modèles annoncent un ordre de grandeur plutôt qu'une liste ;
    /// afficher « 31 langues » pour un modèle qui en revendique six cents
    /// serait faux dans l'autre sens.
    pub languages_label: &'static str,
    /// Taille annoncée, relevée sur le dépôt.
    pub bytes: u64,
    /// Recommandé comme premier modèle.
    pub recommended: bool,
    /// Ce que la lecture donnera : fluide, ou hachée parce que le modèle
    /// produit moins vite qu'il ne parle. Mesuré, pas promis.
    pub pace: &'static str,
    /// Fichiers téléchargés à côté du fichier principal.
    pub companions: &'static [Companion],
    pub runtime: Runtime,
    /// Les voix, pour la fiche (« Française : … · Anglaise : … »).
    pub voices_label: &'static str,
    /// Les licences, pour la fiche.
    pub licenses: &'static str,
}

impl ModelPackage {
    /// Taille annoncée du paquet entier, compagnons compris.
    pub fn total_bytes(&self) -> u64 {
        self.bytes + self.companions.iter().map(|c| c.bytes).sum::<u64>()
    }

    /// Nom du fichier une fois installé.
    pub fn file_name(&self) -> &str {
        self.file.rsplit('/').next().unwrap_or(self.file)
    }

    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    pub fn speaks(&self, language: &str) -> bool {
        let code = language.split(['-', '_']).next().unwrap_or(language);
        self.languages.contains(&code)
    }
}

const KOKORO_MLX_REPO: &str = "mlx-community/Kokoro-82M-bf16";
const KOKORO_MLX_REVISION: &str = "a71e4d38b236d968966a2002c4c895dbd12b1c3c";
const KOKORO_ONNX_REPO: &str = "onnx-community/Kokoro-82M-v1.0-ONNX-timestamped";
const KOKORO_ONNX_REVISION: &str = "dd4401a9add81ac692d20e240d22ec9dda82cc29";
const KOKORO_LANGUAGES: &[&str] = &["fr", "en"];
/// Une voix Kokoro : 510 × 256 flottants, en safetensors.
pub const KOKORO_VOICE_BYTES: u64 = 522_320;

macro_rules! kokoro_voice {
    ($name:literal) => {
        Companion {
            repo: KOKORO_MLX_REPO,
            revision: KOKORO_MLX_REVISION,
            file: concat!("voices/", $name, ".safetensors"),
            dest: concat!("voices/", $name, ".safetensors"),
            bytes: KOKORO_VOICE_BYTES,
        }
    };
}

/// Ce que les deux paquets Kokoro partagent : la configuration (elle porte
/// le vocabulaire des phonèmes) et les voix. `p-fenrir60` n'y est pas : elle
/// est livrée avec Sillon.
///
/// Les 29 voix du paquet restent téléchargées alors que quatre seulement
/// sont proposées (`KOKORO_VOICES`) : `ff_siwis` et `am_fenrir` refont
/// `p-fenrir60` (`examples/kokoro_blend.rs`), et une installation déjà faite
/// garde le même jeu de fichiers — rien à migrer, rien d'orphelin à la
/// suppression.
const KOKORO_SHARED: &[Companion] = &[
    Companion {
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "config.json",
        dest: "config.json",
        bytes: 2_351,
    },
    kokoro_voice!("ff_siwis"),
    kokoro_voice!("af_bella"),
    kokoro_voice!("af_alloy"),
    kokoro_voice!("af_aoede"),
    kokoro_voice!("af_heart"),
    kokoro_voice!("af_jessica"),
    kokoro_voice!("af_kore"),
    kokoro_voice!("af_nicole"),
    kokoro_voice!("af_nova"),
    kokoro_voice!("af_river"),
    kokoro_voice!("af_sarah"),
    kokoro_voice!("af_sky"),
    kokoro_voice!("am_adam"),
    kokoro_voice!("am_echo"),
    kokoro_voice!("am_eric"),
    kokoro_voice!("am_fenrir"),
    kokoro_voice!("am_liam"),
    kokoro_voice!("am_michael"),
    kokoro_voice!("am_onyx"),
    kokoro_voice!("am_puck"),
    kokoro_voice!("am_santa"),
    kokoro_voice!("bf_alice"),
    kokoro_voice!("bf_emma"),
    kokoro_voice!("bf_isabella"),
    kokoro_voice!("bf_lily"),
    kokoro_voice!("bm_daniel"),
    kokoro_voice!("bm_fable"),
    kokoro_voice!("bm_george"),
    kokoro_voice!("bm_lewis"),
];

const KOKORO_VOICES_LABEL: &str = "Française : p-fenrir60 · Anglaise : af_bella, am_fenrir, af_heart";

/// Modèles proposés à l'installation : Kokoro seul, en MLX (l'app) ou en
/// int8 par ONNX Runtime (le terminal). Sans lui, la voix du système lit.
///
/// Supertonic et MagpieTTS n'y sont plus (14 oct. 2026) : un seul modèle,
/// c'est un choix de moins à faire. Le moteur audio.cpp reste compilable, et
/// un `model_path` posé à la main le charge encore.
///
/// Restreint aux familles réellement compilées dans le binaire : proposer un
/// modèle que le moteur ne saurait pas charger n'aiderait personne.
pub const CATALOG: &[ModelPackage] = &[
    ModelPackage {
        id: "kokoro_82m_mlx",
        family: "kokoro",
        display_name: "Kokoro 82M · MLX",
        description: "Voix naturelle et légère, calculée par la puce graphique.",
        precision: "bf16",
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "kokoro-v1_0.safetensors",
        languages_label: "Français, anglais",
        bytes: 327_115_152,
        recommended: true,
        pace: "Quinze fois plus vite que la parole sur M5 : lecture fluide.",
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroMlx,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: "Poids et voix Apache-2.0 · code MLX MIT · espeak-ng GPL-3.0 (programme séparé)",
    },
    ModelPackage {
        id: "kokoro_82m_int8",
        family: "kokoro",
        display_name: "Kokoro 82M · int8",
        description: "Voix naturelle et légère, calculée par le processeur (ONNX Runtime ≥ 1.31).",
        precision: "int8",
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_ONNX_REPO,
        revision: KOKORO_ONNX_REVISION,
        file: "onnx/model_quantized.onnx",
        languages_label: "Français, anglais",
        bytes: 92_361_055,
        recommended: true,
        pace: "Huit à dix fois plus vite que la parole sur processeur : lecture fluide.",
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroOnnx,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: "Poids et voix Apache-2.0 · ONNX Runtime MIT · espeak-ng GPL-3.0 (programme séparé)",
    },
];

/// Familles compilées dans ce binaire, renseignées par `build.rs`.
pub fn compiled_families() -> Vec<&'static str> {
    option_env!("SILLON_COMPILED_FAMILIES")
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect()
}

/// Catalogue restreint à ce que ce binaire sait charger.
///
/// audio.cpp : les familles compilées. Kokoro MLX : seulement quand l'app
/// fournit le calcul (`mlx`), et alors jamais le paquet ONNX à côté. Kokoro
/// int8 : seulement avec la feature `kokoro-onnx`.
pub fn available_with(mlx: bool) -> Vec<&'static ModelPackage> {
    let families = compiled_families();
    CATALOG
        .iter()
        .filter(|p| match p.runtime {
            Runtime::AudioCpp => families.contains(&p.family),
            Runtime::KokoroMlx => mlx,
            Runtime::KokoroOnnx => !mlx && cfg!(feature = "kokoro-onnx"),
        })
        .collect()
}

/// Le catalogue du terminal : sans calcul MLX.
pub fn available() -> Vec<&'static ModelPackage> {
    available_with(false)
}

/// Répertoire où sont rangés les modèles installés.
pub fn models_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("models"))
}

pub fn install_path(package: &ModelPackage) -> Option<PathBuf> {
    models_dir().map(|d| main_path_in(&d, package))
}

fn main_path_in(root: &Path, package: &ModelPackage) -> PathBuf {
    root.join(package.family).join(package.file_name())
}

fn companion_path_in(root: &Path, package: &ModelPackage, companion: &Companion) -> PathBuf {
    root.join(package.family).join(companion.dest)
}

/// Ce qu'il faut télécharger pour ce paquet : adresse, destination, taille.
pub(crate) fn planned_files_in(root: &Path, package: &ModelPackage) -> Vec<(String, PathBuf, u64)> {
    let mut files = vec![(package.url(), main_path_in(root, package), package.bytes)];
    files.extend(
        package
            .companions
            .iter()
            .map(|c| (c.url(), companion_path_in(root, package, c), c.bytes)),
    );
    files
}

/// Vrai si le `model_path` configuré désigne ce fichier installé.
///
/// Le moteur développe le `~` et suit les liens : une configuration écrite à
/// la main (`~/…`, ou via un dossier lié) désigne le même fichier qu'un chemin
/// absolu. Une simple égalité de textes laisserait supprimer le modèle en
/// cours d'usage.
pub fn is_active_path(configured: &str, installed: &Path) -> bool {
    is_active_path_with(configured, installed, dirs::home_dir().as_deref())
}

fn is_active_path_with(configured: &str, installed: &Path, home: Option<&Path>) -> bool {
    if configured.is_empty() {
        return false;
    }
    let expanded = PathBuf::from(crate::config::expand_tilde_with(configured, home));
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(&expanded) == canonical(installed)
}

/// Vrai si `configured` désigne le fichier installé de ce modèle.
pub fn is_active_package(configured: &str, package: &ModelPackage) -> bool {
    install_path(package).is_some_and(|p| is_active_path(configured, &p))
}

/// Taille sur disque du fichier principal, s'il est là.
///
/// Un téléchargement interrompu laisse un `.part`, qui n'est jamais confondu
/// avec un modèle complet.
pub fn installed_size(package: &ModelPackage) -> Option<u64> {
    let path = install_path(package)?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn size_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Installé : chaque fichier, le principal comme ses compagnons, a sa taille
/// attendue. Un principal tronqué n'est pas installé : la migration ne doit
/// pas basculer dessus.
pub fn is_installed_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)) == Some(package.bytes)
        && package
            .companions
            .iter()
            .all(|c| size_of(&companion_path_in(root, package, c)) == Some(c.bytes))
}

pub fn is_installed(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_installed_in(&root, package))
}

/// Vrai si un fichier du paquet est là sans avoir la taille annoncée.
///
/// Un téléchargement interrompu ailleurs que par nous, un disque plein, une
/// copie tronquée : le fichier existe et se présente comme installé, puis le
/// moteur refuse de l'ouvrir sur un message illisible. Mieux vaut le dire.
pub fn is_truncated_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)).is_some_and(|s| s != package.bytes)
        || package.companions.iter().any(|c| {
            size_of(&companion_path_in(root, package, c)).is_some_and(|s| s != c.bytes)
        })
}

pub fn is_truncated(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_truncated_in(&root, package))
}

/// Premier modèle installé, celui que le lecteur choisit par défaut.
pub fn first_installed() -> Option<&'static ModelPackage> {
    first_installed_with(false)
}

/// Comme `first_installed`, pour un lecteur dont l'app fournit (ou non) le
/// calcul MLX.
pub fn first_installed_with(mlx: bool) -> Option<&'static ModelPackage> {
    available_with(mlx).into_iter().find(|p| is_installed(p))
}

/// Octets présents sur disque pour ce paquet : fichier principal et compagnons.
pub fn installed_bytes_in(root: &Path, package: &ModelPackage) -> u64 {
    std::iter::once(main_path_in(root, package))
        .chain(package.companions.iter().map(|c| companion_path_in(root, package, c)))
        .filter_map(|p| size_of(&p))
        .sum()
}

pub fn installed_bytes(package: &ModelPackage) -> u64 {
    models_dir().map_or(0, |root| installed_bytes_in(&root, package))
}

/// Vrai si un fichier du paquet, ou un `.part` de lui, traîne sur le disque :
/// de quoi proposer de le retirer même incomplet.
///
/// Un compagnon qu'un autre paquet de la famille, présent, nomme aussi ne
/// compte pas : il n'est pas à ce paquet seul, et `remove_in` le garderait.
pub fn has_files_in(root: &Path, package: &ModelPackage) -> bool {
    let present = |p: &Path| p.exists() || p.with_extension("part").exists();
    present(&main_path_in(root, package))
        || package.companions.iter().any(|c| {
            !shared_with_sibling(root, package, c) && present(&companion_path_in(root, package, c))
        })
}

/// Un autre paquet de la famille nomme ce compagnon et est là : installé, ou
/// en cours de téléchargement (son fichier principal ou son `.part`).
fn shared_with_sibling(root: &Path, package: &ModelPackage, companion: &Companion) -> bool {
    CATALOG.iter().any(|other| {
        let main = main_path_in(root, other);
        other.id != package.id
            && other.family == package.family
            && (main.exists() || main.with_extension("part").exists())
            && other.companions.iter().any(|c| c.dest == companion.dest)
    })
}

pub fn has_files(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| has_files_in(&root, package))
}

/// Le dossier d'un paquet Kokoro complet, d'après son fichier principal :
/// celui où se trouvent `config.json` et `voices/`.
pub fn kokoro_dir(model_path: &str) -> Option<PathBuf> {
    let dir = Path::new(model_path).parent()?;
    (Path::new(model_path).is_file() && dir.join("config.json").is_file()).then(|| dir.to_path_buf())
}

pub fn remove(package: &ModelPackage) -> Result<()> {
    let root = models_dir().context("répertoire de données introuvable")?;
    remove_in(&root, package)
}

/// Supprime un paquet. Un compagnon partagé reste tant qu'un autre paquet de
/// la famille, présent, le nomme : retirer le paquet MLX ne casse pas l'int8.
pub fn remove_in(root: &Path, package: &ModelPackage) -> Result<()> {
    let path = main_path_in(root, package);
    // Un paquet interrompu n'a pas forcément tous ses fichiers : l'absence
    // n'arrête pas le nettoyage. Les `.part` partent aussi.
    let mut first_error = None;
    for file in [path.clone(), path.with_extension("part")] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                first_error.get_or_insert_with(|| anyhow!("suppression de {} : {e}", file.display()));
            }
        }
    }
    for companion in package.companions {
        if !shared_with_sibling(root, package, companion) {
            let file = companion_path_in(root, package, companion);
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(file.with_extension("part"));
        }
    }
    // Laisse le dossier de famille propre s'il devient vide.
    let family = root.join(package.family);
    let _ = std::fs::remove_dir(family.join("voices"));
    let _ = std::fs::remove_dir(&family);
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Espace disque libre du volume des modèles, en octets.
pub fn free_space() -> Option<u64> {
    let dir = models_dir()?;
    // Le répertoire peut ne pas exister encore : on interroge son ancêtre.
    let probe = dir.ancestors().find(|p| p.exists())?;
    free_space_at(probe)
}

#[cfg(unix)]
fn free_space_at(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY : `statvfs` remplit une structure que nous fournissons, et le
    // chemin est une chaîne C valide.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_space_at(_path: &Path) -> Option<u64> {
    None
}

/// État d'un téléchargement, consulté par l'interface à chaque trame.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Running { received: u64, total: u64 },
    Done,
    Failed(String),
    Cancelled,
}

/// Téléchargement en cours, annulable.
pub struct Download {
    pub package: &'static ModelPackage,
    received: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    outcome: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
}

impl Download {
    /// Lance le téléchargement dans un thread dédié : fichier principal puis
    /// compagnons, en reprenant ce qui est déjà là.
    pub fn start(package: &'static ModelPackage) -> Result<Self> {
        let root = models_dir().context("répertoire de données introuvable")?;
        Self::start_in(package, planned_files_in(&root, package))
    }

    /// Télécharge ces fichiers (adresse, destination, taille) pour ce paquet.
    pub(crate) fn start_in(package: &'static ModelPackage, files: Vec<(String, PathBuf, u64)>) -> Result<Self> {
        let received = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(package.total_bytes()));
        let outcome = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = (Arc::clone(&received), Arc::clone(&outcome), Arc::clone(&cancel));
        thread::spawn(move || {
            let (received, outcome, cancel) = worker;
            let mut base = 0u64;
            let mut result = Ok(());
            for (url, destination, bytes) in files {
                if size_of(&destination) == Some(bytes) {
                    base += bytes;
                    received.store(base, Ordering::Relaxed);
                    continue;
                }
                result = fetch(&url, &destination, bytes, &received, base, &cancel);
                if result.is_err() || cancel.load(Ordering::Relaxed) {
                    break;
                }
                base += bytes;
            }
            let state = match result {
                Ok(()) if cancel.load(Ordering::Relaxed) => Progress::Cancelled,
                Ok(()) => Progress::Done,
                Err(err) => Progress::Failed(err.to_string()),
            };
            *outcome.lock().unwrap() = Some(state);
        });

        Ok(Self { package, received, total, outcome, cancel })
    }

    pub fn progress(&self) -> Progress {
        if let Some(state) = self.outcome.lock().unwrap().clone() {
            return state;
        }
        Progress::Running {
            received: self.received.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }

    /// Un téléchargement sans réseau ni fil, pour les tests.
    #[cfg(test)]
    pub(crate) fn fake(package: &'static ModelPackage, finished: bool) -> Self {
        Self {
            package,
            received: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(package.total_bytes())),
            outcome: Arc::new(Mutex::new(finished.then_some(Progress::Done))),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Un téléchargement fini sur cette issue (échec, annulation…), pour les
    /// tests.
    #[cfg(test)]
    pub(crate) fn fake_ended(package: &'static ModelPackage, ending: Progress) -> Self {
        let download = Self::fake(package, false);
        *download.outcome.lock().unwrap() = Some(ending);
        download
    }

    /// Vrai si l'arrêt a été demandé.
    #[cfg(test)]
    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.outcome.lock().unwrap().is_some()
    }
}

/// Télécharge `url` vers `destination`, en reprenant un transfert interrompu.
///
/// L'écriture passe par un fichier `.part` renommé à la fin : un modèle présent
/// dans le répertoire est donc toujours un modèle complet.
///
/// `base` est ce que les fichiers précédents du paquet ont déjà reçu : la
/// progression est celle du paquet entier.
fn fetch(
    url: &str,
    destination: &Path,
    expected: u64,
    received: &AtomicU64,
    base: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    fetch_with(url, destination, expected, received, base, cancel, &|| {})
}

/// `fetch`, avec un point d'accroche juste avant que le `.part` soit créé.
///
/// Entre la première création du dossier et la création du `.part`, une
/// requête réseau s'écoule : un `models::remove` concurrent peut y retirer un
/// dossier de famille devenu vide. Le dossier est donc recréé au dernier
/// moment, et le test d'accroche le retire exactement là.
fn fetch_with(
    url: &str,
    destination: &Path,
    expected: u64,
    received: &AtomicU64,
    base: u64,
    cancel: &AtomicBool,
    before_create: &dyn Fn(),
) -> Result<()> {
    let ensure_parent = || -> Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("création de {}", parent.display()))?;
        }
        Ok(())
    };
    ensure_parent()?;
    let partial = destination.with_extension("part");
    let already = match resume_from(&partial, expected) {
        // Tout est déjà là : le serveur répondrait 416 à une reprise.
        Resume::Complete => {
            received.store(base + expected, Ordering::Relaxed);
            before_create();
            ensure_parent()?;
            return settle(&partial, destination, expected);
        }
        Resume::At(bytes) => bytes,
        Resume::FromScratch => 0,
    };

    // Un serveur qui ne répond plus ne doit pas figer l'installation : sans
    // délai, `d` restait sans effet et toute autre installation refusée.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();
    let mut request = agent.get(url);
    if already > 0 {
        // Reprend là où le transfert précédent s'est arrêté.
        request = request.set("Range", &format!("bytes={already}-"));
    }
    let response = request.call().map_err(|e| anyhow!("{e}"))?;

    let resuming = response.status() == 206;
    before_create();
    ensure_parent()?;
    let mut file = if resuming {
        received.store(base + already, Ordering::Relaxed);
        std::fs::OpenOptions::new().append(true).open(&partial)?
    } else {
        received.store(base, Ordering::Relaxed);
        std::fs::File::create(&partial)?
    };

    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = if resuming { already } else { 0 };

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Le `.part` est conservé : une reprise repartira de là.
            return Ok(());
        }
        let read = reader.read(&mut buffer).context("lecture du flux réseau")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).context("écriture sur disque")?;
        written += read as u64;
        received.store(base + written, Ordering::Relaxed);
    }
    file.flush()?;
    drop(file);

    settle(&partial, destination, expected)
}

/// Ce qu'un `.part` laissé par un transfert précédent permet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resume {
    /// Rien : il n'y en a pas, ou il ne vient pas du même fichier.
    FromScratch,
    /// Reprendre à cet octet.
    At(u64),
    /// Tout est déjà là.
    Complete,
}

/// Décide de la reprise d'après la taille du `.part` et celle qu'on attend.
///
/// Un `.part` plus long que le fichier annoncé ne vient pas de lui : il est
/// retiré, sans quoi la reprise le prolongerait sans fin.
fn resume_from(partial: &Path, expected: u64) -> Resume {
    let Ok(len) = std::fs::metadata(partial).map(|m| m.len()) else {
        return Resume::FromScratch;
    };
    if expected > 0 && len == expected {
        Resume::Complete
    } else if expected > 0 && len > expected {
        let _ = std::fs::remove_file(partial);
        Resume::FromScratch
    } else if len > 0 {
        Resume::At(len)
    } else {
        Resume::FromScratch
    }
}

/// Met en place un transfert terminé, si sa taille est celle annoncée.
///
/// Un fichier plus court vient d'un disque plein ou d'un serveur qui a coupé
/// court ; un fichier plus long, d'une reprise sur un blob qui a changé entre
/// temps. Dans les deux cas le moteur refuserait de l'ouvrir sur un message
/// illisible, et le `.part` ne peut plus servir de base : il est retiré.
fn settle(partial: &Path, destination: &Path, expected: u64) -> Result<()> {
    let len = std::fs::metadata(partial)
        .with_context(|| format!("lecture de {}", partial.display()))?
        .len();
    if expected > 0 && len != expected {
        let _ = std::fs::remove_file(partial);
        anyhow::bail!(
            "fichier reçu de {} au lieu de {} : le transfert est à refaire",
            human_size(len),
            human_size(expected)
        );
    }
    std::fs::rename(partial, destination)
        .with_context(|| format!("installation de {}", destination.display()))?;
    Ok(())
}

/// Formate un nombre d'octets pour l'affichage.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [(&str, u64); 3] = [("Go", 1 << 30), ("Mo", 1 << 20), ("Ko", 1 << 10)];
    for (unit, scale) in UNITS {
        if bytes >= scale {
            // Virgule décimale, à la française.
            return format!("{:.1} {unit}", bytes as f64 / scale as f64).replace('.', ",");
        }
    }
    format!("{bytes} o")
}

/// Une voix intégrée, proposée au choix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: &'static str,
    pub label: &'static str,
    /// Langue de la voix ; vide : toutes celles du modèle.
    pub language: &'static str,
}

/// Voix proposées pour une famille donnée.
///
/// Vide quand la famille n'expose pas de voix nommées : le champ redevient
/// alors libre, pour y mettre un chemin de WAV à cloner.
pub fn voices_for_family(family: &str) -> &'static [VoiceOption] {
    match family {
        "kokoro" => KOKORO_VOICES,
        _ => &[],
    }
}

/// Voix de Kokoro : une française, trois anglaises (14 oct. 2026 : seules
/// les voix retenues à l'écoute ; les autres restent dans le paquet, sans
/// être proposées).
///
/// `p-fenrir60` est livrée avec Sillon (timbre de `ff_siwis`, prosodie mêlée à
/// celle d'`am_fenrir`, §11 du rapport) ; les autres viennent du paquet. La
/// première de chaque langue est sa voix par défaut : une voix configurée qui
/// n'est plus listée retombe sur elle.
const KOKORO_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr" },
    VoiceOption { id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en" },
    VoiceOption { id: "am_fenrir", label: "am_fenrir · américaine, homme", language: "en" },
    VoiceOption { id: "af_heart", label: "af_heart · américaine, femme", language: "en" },
];

/// Sessions de synthèse qu'une famille demande pour lire d'un trait.
///
/// Une session pèse environ 460 Mo — deux fois le modèle — et s'ouvre après
/// les autres, derrière le verrou de cycle de vie. En ouvrir plus que
/// nécessaire coûte donc de la mémoire *et* retarde le premier son.
pub fn sessions_for_family(family: &str) -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    match family {
        // Mesuré à vingt fois le temps réel sur une session : elle reste loin
        // devant la lecture, et les sauts n'attendent que 0,25 s.
        "supertonic" => 1,
        // MagpieTTS sature le GPU à lui seul : trois sessions produisent six
        // phrases en 29,4 s, une seule en 29,6 s (M5, Metal). Le débit est le
        // même, mais à trois les phrases se partagent le GPU et celle dont la
        // lecture a besoin arrive tard — la deuxième à 29 s au lieu de 13.
        // Une session : le silence après la première phrase passe de 23 s à 7.
        "magpie_tts" => 1,
        // Kokoro calcule en série (MLX comme ORT) : une session.
        "kokoro" => 1,
        // Les familles inconnues sont supposées lentes et parallélisables, la
        // synthèse en avance valant mieux qu'un silence.
        _ => (cores / 3).clamp(2, 4),
    }
}

/// Réglages de synthèse réellement honorés par une famille.
///
/// Toutes les familles n'en lisent qu'une partie. Afficher un réglage sans
/// effet donnerait l'illusion d'agir sur la synthèse — et l'aperçu, lui, ne
/// ment pas : mieux vaut dire d'avance ce qui restera sans suite.
pub fn honoured_options(family: &str) -> &'static [&'static str] {
    match family {
        // Relevé sur `generation_options_from_request` de Supertonic : tout le
        // reste — hauteur, énergie, intonation, consigne — est ignoré.
        "supertonic" => &[
            "voice",
            "language",
            "speaking_rate",
            "seed",
            "num_inference_steps",
        ],
        // Relevé sur les options de requête de MagpieTTS. Il n'expose aucun
        // contrôle de débit — la vitesse de lecture, appliquée au son, y vaut
        // quand même. Il valide par ailleurs
        // sa requête et refuse toute clé absente de cette liste.
        "magpie_tts" => &[
            "voice",
            "voice_id",
            "speaker",
            "language",
            "temperature",
            "top_k",
            "guidance_scale",
            "seed",
        ],
        // Kokoro : la voix et la langue ; la vitesse s'applique au son.
        "kokoro" => &["voice", "language"],
        // Famille inconnue : on n'affirme rien, tout est présenté comme possible.
        _ => &[],
    }
}

/// Voix par défaut d'une famille, quand elle en nomme.
pub fn default_voice(family: &str) -> &'static str {
    voices_for_family(family).first().map_or("", |v| v.id)
}

/// Vrai quand la famille choisit sa voix selon la langue du document.
pub fn voices_follow_language(family: &str) -> bool {
    family == "kokoro"
}

fn language_code(language: &str) -> &str {
    crate::text::language::Lang::parse(language).map_or(language, |l| l.code())
}

/// Voix par défaut d'une famille dans une langue ; sa voix par défaut tout
/// court si aucune n'est rangée sous cette langue.
pub fn default_voice_for_language(family: &str, language: &str) -> &'static str {
    let code = language_code(language);
    voices_for_family(family)
        .iter()
        .find(|v| v.language == code)
        .map_or_else(|| default_voice(family), |v| v.id)
}

/// La voix qui parle `language` : celle de la configuration si la famille la
/// connaît dans cette langue, sinon sa voix par défaut — sans réécrire la
/// configuration (règle des valeurs non listées).
pub fn voice_for_language(family: &str, language: &str, configured: Option<&str>) -> &'static str {
    let code = language_code(language);
    configured
        .map(str::trim)
        .and_then(|c| {
            voices_for_family(family)
                .iter()
                .find(|v| v.id == c && (v.language.is_empty() || v.language == code))
        })
        .map_or_else(|| default_voice_for_language(family, code), |v| v.id)
}

/// Orthographes possibles de l'option qui porte le nom de la voix.
///
/// La plupart des familles reçoivent la voix par le conditionnement de
/// locuteur du shim (`cached_voice_id`). MagpieTTS fait exception : il ne la
/// lit que dans les options de requête, et l'y omettre fait parler ses cinq
/// locuteurs d'une seule et même voix, sans le moindre message d'erreur.
///
/// Le nom exact dépend du **paquet** et non de la famille — deux variantes du
/// même modèle n'embarquent pas la même spécification — d'où une liste de
/// candidats plutôt qu'une valeur unique.
pub fn voice_option_keys(family: &str) -> &'static [&'static str] {
    match family {
        // Deux orthographes coexistent selon la version du paquet : la
        // variante quantifiée attend `voice_id`, celle en précision d'origine
        // `speaker`. La session essaie l'une puis l'autre.
        "magpie_tts" => &["voice_id", "speaker"],
        _ => &[],
    }
}

/// Vrai si la famille prend en compte ce réglage.
pub fn honours(family: &str, option: &str) -> bool {
    let honoured = honoured_options(family);
    honoured.is_empty() || honoured.contains(&option)
}

/// Langues proposées quand la famille n'est pas au catalogue : les sept que
/// le terminal proposait.
pub const FALLBACK_LANGUAGES: &[&str] = &["fr", "en", "de", "es", "it", "pt", "nl"];

/// Langues qu'une famille sait parler : celles de son paquet au catalogue.
///
/// Toutes les variantes d'une famille partagent la même liste ; le catalogue
/// entier est consulté, pas seulement ce que ce binaire sait charger, pour
/// que l'écran dise juste même compilé sans audio.cpp.
pub fn languages_for_family(family: &str) -> Vec<&'static str> {
    CATALOG
        .iter()
        .find(|p| p.family == family)
        .map(|p| p.languages.to_vec())
        .unwrap_or_else(|| FALLBACK_LANGUAGES.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_follow_the_family() {
        assert_eq!(languages_for_family("kokoro"), ["fr", "en"]);
        // Une famille retirée du catalogue est une famille inconnue.
        assert_eq!(languages_for_family("supertonic"), FALLBACK_LANGUAGES.to_vec());
        // Famille inconnue : les sept que le terminal proposait.
        assert_eq!(languages_for_family(""), FALLBACK_LANGUAGES.to_vec());
    }

    /// Retrouve une entrée par son identifiant, plutôt que par sa position :
    /// l'ordre du catalogue suit la recommandation et change avec elle.
    fn package(id: &str) -> &'static ModelPackage {
        CATALOG
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} absent du catalogue"))
    }

    #[test]
    fn magpie_gets_a_single_session_and_supertonic_too() {
        assert_eq!(sessions_for_family("magpie_tts"), 1);
        assert_eq!(sessions_for_family("supertonic"), 1);
        assert!(sessions_for_family("inconnue") >= 2);
    }

    #[test]
    fn urls_follow_the_huggingface_layout() {
        assert_eq!(
            package("kokoro_82m_mlx").url(),
            "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/kokoro-v1_0.safetensors"
        );
        assert_eq!(
            package("kokoro_82m_int8").url(),
            "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX-timestamped/resolve/dd4401a9add81ac692d20e240d22ec9dda82cc29/onnx/model_quantized.onnx"
        );
    }

    #[test]
    fn file_name_is_extracted_from_the_repo_path() {
        assert_eq!(package("kokoro_82m_mlx").file_name(), "kokoro-v1_0.safetensors");
        assert_eq!(package("kokoro_82m_int8").file_name(), "model_quantized.onnx");
    }

    #[test]
    fn every_catalog_entry_speaks_french() {
        // Le lecteur s'adresse d'abord à un usage francophone.
        for package in CATALOG {
            assert!(package.speaks("fr"), "{} ne parle pas français", package.id);
            assert!(package.speaks("fr-FR"), "variante régionale rejetée");
        }
    }

    #[test]
    fn kokoro_is_the_recommended_voice_wherever_it_runs() {
        assert!(CATALOG.iter().filter(|p| p.recommended).all(|p| p.family == "kokoro"));
        for mlx in [true, false] {
            let recommended = available_with(mlx).into_iter().filter(|p| p.recommended).count();
            assert!(recommended <= 1, "deux paquets recommandés côte à côte (mlx = {mlx})");
        }
    }

    /// Un seul modèle au catalogue : Kokoro, en MLX ou en int8. Supertonic et
    /// MagpieTTS n'y sont plus, même compilés.
    #[test]
    fn kokoro_is_the_only_catalogued_model() {
        let ids: Vec<&str> = CATALOG.iter().map(|p| p.id).collect();
        assert_eq!(ids, ["kokoro_82m_mlx", "kokoro_82m_int8"]);
        assert!(available().iter().chain(available_with(true).iter()).all(|p| p.family == "kokoro"));
    }

    #[test]
    fn the_mlx_package_needs_the_app_and_never_sits_next_to_onnx() {
        let with_mlx: Vec<&str> = available_with(true).iter().map(|p| p.id).collect();
        assert!(with_mlx.contains(&"kokoro_82m_mlx"));
        assert!(!with_mlx.contains(&"kokoro_82m_int8"), "pas d'ORT dans l'app");
        let without: Vec<&str> = available_with(false).iter().map(|p| p.id).collect();
        assert!(!without.contains(&"kokoro_82m_mlx"));
        assert_eq!(without.contains(&"kokoro_82m_int8"), cfg!(feature = "kokoro-onnx"));
    }

    #[test]
    fn kokoro_packages_are_pinned_and_carry_their_voices() {
        let mlx = package("kokoro_82m_mlx");
        assert_eq!(mlx.revision, "a71e4d38b236d968966a2002c4c895dbd12b1c3c");
        assert_eq!(mlx.bytes, 327_115_152);
        let int8 = package("kokoro_82m_int8");
        assert_eq!(int8.revision, "dd4401a9add81ac692d20e240d22ec9dda82cc29");
        assert_eq!(int8.file, "onnx/model_quantized.onnx");
        assert_eq!(int8.bytes, 92_361_055);
        // config.json et 29 voix : ff_siwis et les 28 anglaises.
        assert_eq!(mlx.companions.len(), 30);
        assert_eq!(mlx.companions, int8.companions, "voix et configuration communes");
        assert_eq!(mlx.total_bytes(), 327_115_152 + 2_351 + 29 * 522_320);
        assert!(mlx.companions.iter().any(|c| c.dest == "voices/af_bella.safetensors"));
        assert!(mlx.companions.iter().all(|c| c.repo == "mlx-community/Kokoro-82M-bf16"));
        assert_eq!(
            mlx.companions[0].url(),
            "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/config.json"
        );
    }

    #[test]
    fn kokoro_reads_only_its_voice_and_language() {
        for ignored in ["pitch_shift", "energy", "emotion", "temperature", "seed", "instruction"] {
            assert!(!honours("kokoro", ignored), "{ignored}");
        }
        assert!(honours("kokoro", "voice") && honours("kokoro", "language"));
        assert_eq!(sessions_for_family("kokoro"), 1);
        assert_eq!(languages_for_family("kokoro"), ["fr", "en"]);
    }

    /// Crée les fichiers d'un paquet, creux (taille annoncée, aucun octet
    /// écrit) : 327 Mo ne coûtent rien sur APFS ni ext4.
    fn sparse(root: &Path, package: &ModelPackage, with_companions: bool) {
        let mut files = vec![(main_path_in(root, package), package.bytes)];
        if with_companions {
            files.extend(package.companions.iter().map(|c| (companion_path_in(root, package, c), c.bytes)));
        }
        for (path, bytes) in files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        }
    }

    #[test]
    fn a_package_is_installed_only_when_every_file_is_there() {
        let root = scratch("complet-kokoro");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, false);
        assert!(!is_installed_in(&root, mlx), "voix absentes");
        sparse(&root, mlx, true);
        assert!(is_installed_in(&root, mlx));
        let voice = companion_path_in(&root, mlx, &mlx.companions[3]);
        std::fs::File::create(&voice).unwrap().set_len(10).unwrap();
        assert!(!is_installed_in(&root, mlx), "une voix tronquée");
        assert!(is_truncated_in(&root, mlx));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_truncated_main_file_is_not_installed_and_the_migration_does_not_switch() {
        use crate::reader::migration::{plan, Step};
        let root = scratch("principal-tronque");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, true);
        std::fs::File::create(main_path_in(&root, mlx)).unwrap().set_len(1_000).unwrap();
        assert!(!is_installed_in(&root, mlx), "le modèle principal est tronqué");
        assert!(is_truncated_in(&root, mlx));
        let mut config = crate::config::Config::default();
        config.engine.family = "supertonic".into();
        let step = plan(&config, &[mlx], |p| is_installed_in(&root, p));
        assert_eq!(step, Step::Offer(mlx), "proposé, pas basculé sur un fichier tronqué");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_one_kokoro_package_keeps_what_the_other_needs() {
        let root = scratch("partage-kokoro");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, mlx, true);
        sparse(&root, int8, true);
        remove_in(&root, mlx).unwrap();
        assert!(!main_path_in(&root, mlx).exists());
        assert!(is_installed_in(&root, int8), "voix et configuration restent pour l'autre");
        remove_in(&root, int8).unwrap();
        assert!(!companion_path_in(&root, int8, &int8.companions[0]).exists(), "plus personne n'en a besoin");
        assert!(!root.join("kokoro").exists(), "dossier de famille retiré");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_kokoro_dir_is_one_with_its_configuration() {
        let root = scratch("dossier-kokoro");
        let main = root.join("kokoro").join("kokoro-v1_0.safetensors");
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        std::fs::write(&main, b"x").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), None, "config.json manque");
        std::fs::write(root.join("kokoro").join("config.json"), b"{}").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), Some(root.join("kokoro")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_paths_are_scoped_by_family() {
        // Deux familles peuvent nommer leur fichier pareil : le répertoire les
        // sépare.
        let Some(path) = install_path(package("kokoro_82m_mlx")) else { return };
        assert!(path.ends_with("kokoro/kokoro-v1_0.safetensors"), "{path:?}");
        let Some(path) = install_path(package("kokoro_82m_int8")) else { return };
        assert!(path.ends_with("kokoro/model_quantized.onnx"), "{path:?}");
    }

    #[test]
    fn every_catalogued_family_names_its_voices() {
        // C'est la promesse du catalogue : aucun modèle n'exige qu'on lui
        // fabrique une voix avant de pouvoir l'écouter.
        for package in CATALOG {
            assert!(
                !voices_for_family(package.family).is_empty(),
                "{} n'expose aucune voix nommée",
                package.family
            );
        }
    }

    #[test]
    fn magpie_admits_what_it_leaves_untouched() {
        for honoured in ["voice", "language", "temperature", "top_k", "guidance_scale", "seed"] {
            assert!(honours("magpie_tts", honoured), "{honoured}");
        }
        // Comme Supertonic pour l'instruction, il reçoit sans consulter.
        assert!(!honours("magpie_tts", "speaking_rate"));
        for ignored in ["pitch_shift", "energy", "emotion", "instruction", "top_p"] {
            assert!(!honours("magpie_tts", ignored), "{ignored}");
        }
    }

    #[test]
    fn sizes_are_formatted_readably() {
        assert_eq!(human_size(512), "512 o");
        assert_eq!(human_size(2 * 1024), "2,0 Ko");
        assert_eq!(human_size(312_784_196), "298,3 Mo");
        assert_eq!(human_size(3 * (1 << 30)), "3,0 Go");
    }

    #[test]
    fn unknown_languages_are_rejected() {
        assert!(!CATALOG[0].speaks("zz"));
    }

    #[test]
    fn unknown_families_offer_no_named_voice() {
        assert!(voices_for_family("inconnue").is_empty());
        // Retirées du catalogue, Supertonic et MagpieTTS n'en nomment plus.
        assert!(voices_for_family("supertonic").is_empty());
        assert!(voices_for_family("magpie_tts").is_empty());
    }

    #[test]
    fn setting_support_is_reported_per_family() {
        // Supertonic ignore la température : il ne faut pas la présenter
        // comme agissante.
        assert!(!honours("supertonic", "temperature"));
        assert!(honours("supertonic", "seed"));
        assert!(honours("supertonic", "num_inference_steps"));
        // Famille inconnue : aucune affirmation, tout reste proposé.
        assert!(honours("inconnue", "temperature"));
    }

    #[test]
    fn supertonic_admits_what_it_leaves_untouched() {
        // Ce que le modèle lit vraiment.
        for honoured in ["voice", "language", "speaking_rate", "seed", "num_inference_steps"] {
            assert!(honours("supertonic", honoured), "{honoured}");
        }
        // Ce qu'il reçoit sans jamais le consulter : l'annoncer évite de
        // régler pendant dix minutes une hauteur qui ne bougera pas.
        for ignored in ["pitch_shift", "energy", "emotion", "instruction"] {
            assert!(!honours("supertonic", ignored), "{ignored}");
        }
    }


    #[test]
    fn no_two_packages_of_a_family_are_the_same_file() {
        // « Q8_0 » et « origine » de Supertonic pointaient le même blob sur le
        // dépôt — même taille, même empreinte : deux entrées pour 454 Mo
        // téléchargés deux fois.
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert!(
                    !(a.family == b.family && a.bytes == b.bytes),
                    "{} et {} sont le même fichier",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// Un serveur local qui répond une fois `body`, sans rien d'externe.
    fn serve_once(body: &'static [u8]) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        url
    }

    #[test]
    fn a_family_dir_removed_during_the_request_does_not_fail_the_download() {
        // `remove` retire un dossier de famille vide pendant que la requête
        // réseau est en vol : le `.part` doit quand même pouvoir être créé,
        // pour le fichier principal comme pour un compagnon dans `voices/`.
        for relative in ["modele.bin", "voices/voix.bin"] {
            let root = scratch("course-dossier");
            let family = root.join("famille");
            let destination = family.join(relative);
            let url = serve_once(b"0123456789");
            let received = AtomicU64::new(0);
            let cancel = AtomicBool::new(false);
            fetch_with(&url, &destination, 10, &received, 0, &cancel, &|| {
                std::fs::remove_dir_all(&family).unwrap();
            })
            .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"0123456789");
            assert_eq!(received.load(Ordering::Relaxed), 10);
            std::fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn a_later_file_reports_progress_after_the_earlier_ones() {
        let root = scratch("decalage");
        let destination = root.join("f").join("deux.bin");
        let url = serve_once(b"abcde");
        let received = AtomicU64::new(0);
        fetch(&url, &destination, 5, &received, 100, &AtomicBool::new(false)).unwrap();
        assert_eq!(received.load(Ordering::Relaxed), 105);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_short_companion_shows_in_the_installed_bytes() {
        let root = scratch("octets-presents");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, true);
        assert_eq!(installed_bytes_in(&root, mlx), mlx.total_bytes());
        let voice = companion_path_in(&root, mlx, &mlx.companions[3]);
        std::fs::File::create(&voice).unwrap().set_len(10).unwrap();
        assert_eq!(installed_bytes_in(&root, mlx), mlx.total_bytes() - mlx.companions[3].bytes + 10);
        assert!(is_truncated_in(&root, mlx));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_partial_package_can_be_found_and_removed_whole() {
        let root = scratch("partiel-kokoro");
        let mlx = package("kokoro_82m_mlx");
        assert!(!has_files_in(&root, mlx));
        // Le gros fichier est là, la configuration et les voix non, un .part traîne.
        sparse(&root, mlx, false);
        let part = companion_path_in(&root, mlx, &mlx.companions[1]).with_extension("part");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, b"xx").unwrap();
        assert!(!is_installed_in(&root, mlx));
        assert!(has_files_in(&root, mlx));
        remove_in(&root, mlx).unwrap();
        assert!(!root.join("kokoro").exists(), "plus rien, dossier de famille retiré");
        // Seul un .part : il part aussi, sans fichier principal.
        std::fs::create_dir_all(root.join("kokoro")).unwrap();
        std::fs::write(main_path_in(&root, mlx).with_extension("part"), b"x").unwrap();
        assert!(has_files_in(&root, mlx));
        remove_in(&root, mlx).unwrap();
        assert!(!has_files_in(&root, mlx));
        assert!(!root.join("kokoro").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_a_partial_package_keeps_what_an_installed_one_shares() {
        let root = scratch("partiel-partage");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, int8, true);
        // Le paquet MLX n'a que quelques voix, sans son fichier principal.
        let voice = companion_path_in(&root, mlx, &mlx.companions[1]);
        assert!(voice.exists());
        remove_in(&root, mlx).unwrap();
        assert!(is_installed_in(&root, int8), "l'int8 garde voix et configuration");
        std::fs::remove_dir_all(&root).ok();
    }

    fn files_for(urls: &[(String, &Path, u64)]) -> Vec<(String, PathBuf, u64)> {
        urls.iter().map(|(u, p, b)| (u.clone(), p.to_path_buf(), *b)).collect()
    }

    fn wait(download: &Download) -> Progress {
        for _ in 0..500 {
            if download.is_finished() {
                return download.progress();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("téléchargement jamais terminé");
    }

    #[test]
    fn a_download_skips_files_already_complete_and_reports_the_package() {
        let root = scratch("telechargement");
        let (one, two) = (root.join("f").join("un.bin"), root.join("f").join("voices").join("deux.bin"));
        std::fs::create_dir_all(one.parent().unwrap()).unwrap();
        std::fs::write(&one, b"abc").unwrap();
        // Le premier est déjà là : son adresse ne répond pas et n'est pas appelée.
        let files = files_for(&[("http://127.0.0.1:1/jamais".into(), &one, 3), (serve_once(b"defgh"), &two, 5)]);
        let download = Download::start_in(&CATALOG[0], files).unwrap();
        assert_eq!(wait(&download), Progress::Done);
        assert_eq!(std::fs::read(&two).unwrap(), b"defgh");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_download_stops_at_the_first_failing_file() {
        let root = scratch("telechargement-echec");
        let (one, two, three) = (root.join("f/un.bin"), root.join("f/deux.bin"), root.join("f/trois.bin"));
        // Le deuxième fichier arrive trop court : le troisième n'est pas tenté.
        let files = files_for(&[
            (serve_once(b"abc"), &one, 3),
            (serve_once(b"de"), &two, 5),
            ("http://127.0.0.1:1/jamais".into(), &three, 1),
        ]);
        let download = Download::start_in(&CATALOG[0], files).unwrap();
        assert!(matches!(wait(&download), Progress::Failed(_)));
        assert!(one.exists() && !two.exists() && !three.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    /// Un serveur qui annonce `total` octets, en envoie `first`, le dit sur
    /// `sent`, puis attend `release` avant d'envoyer le reste.
    fn serve_in_two_parts(
        first: &'static [u8],
        rest: &'static [u8],
        sent: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                first.len() + rest.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(first).unwrap();
            stream.flush().unwrap();
            let _ = sent.send(());
            let _ = release.recv();
            let _ = stream.write_all(rest);
        });
        url
    }

    #[test]
    fn a_download_cancelled_mid_package_is_cancelled_and_keeps_its_part() {
        let root = scratch("telechargement-annule");
        let (one, two, three) = (root.join("f/un.bin"), root.join("f/deux.bin"), root.join("f/trois.bin"));
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let files = files_for(&[
            (serve_once(b"abc"), &one, 3),
            (serve_in_two_parts(b"d", b"efgh", sent_tx, release_rx), &two, 5),
            ("http://127.0.0.1:1/jamais".into(), &three, 1),
        ]);
        let download = Download::start_in(&CATALOG[0], files).unwrap();
        sent_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let started = std::time::Instant::now();
        loop {
            match download.progress() {
                Progress::Running { received, .. } if received >= 4 => break,
                other => assert!(started.elapsed().as_secs() < 10, "pas de progression : {other:?}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        download.cancel();
        release_tx.send(()).unwrap();
        assert_eq!(wait(&download), Progress::Cancelled);
        assert_eq!(std::fs::read(&one).unwrap(), b"abc");
        assert!(!two.exists(), "le fichier annulé n'est pas installé");
        assert!(two.with_extension("part").exists(), "le .part reste pour la reprise");
        assert!(!three.exists() && !three.with_extension("part").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn what_a_sibling_shares_is_not_this_packages_to_remove() {
        let root = scratch("partage-presence");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, int8, true);
        assert!(!has_files_in(&root, mlx), "rien qui soit au seul paquet MLX");
        assert!(has_files_in(&root, int8));
        // Le MLX arrive à moitié : son fichier principal le rend visible.
        std::fs::write(main_path_in(&root, mlx).with_extension("part"), b"x").unwrap();
        assert!(has_files_in(&root, mlx));
        // Un frère qui télécharge protège ses compagnons communs.
        remove_in(&root, int8).unwrap();
        assert!(companion_path_in(&root, int8, &int8.companions[0]).exists(), "le MLX en cours les attend");
        assert!(!main_path_in(&root, int8).exists());
        remove_in(&root, mlx).unwrap();
        assert!(!root.join("kokoro").exists(), "plus personne : tout est retiré");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_the_mlx_only_files_leaves_the_shared_ones() {
        let root = scratch("seuls-au-mlx");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, mlx, true);
        std::fs::write(main_path_in(&root, int8), b"x").unwrap(); // l'autre paquet est là, voix partagées
        remove_in(&root, mlx).unwrap();
        assert!(!main_path_in(&root, mlx).exists());
        assert!(companion_path_in(&root, mlx, &mlx.companions[0]).exists());
        std::fs::remove_dir_all(&root).ok();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-models-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_download_of_the_wrong_size_is_refused_and_forgotten() {
        // Une reprise sur un blob qui a changé donne un fichier de la bonne
        // apparence et du mauvais contenu ; un disque plein, un fichier court.
        // La taille annoncée fait foi, et un `.part` faux ne sert pas de base.
        let dir = scratch("tronque");
        let destination = dir.join("m.gguf");
        let partial = dir.join("m.part");
        std::fs::write(&partial, [0u8; 7]).unwrap();
        assert!(settle(&partial, &destination, 10).is_err());
        assert!(!partial.exists(), "le .part faux ne doit pas servir de reprise");
        assert!(!destination.exists());
        std::fs::write(&partial, [0u8; 10]).unwrap();
        settle(&partial, &destination, 10).unwrap();
        assert!(destination.is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_partial_file_already_complete_needs_nothing_from_the_network() {
        // Un `.part` de la taille annoncée n'a plus rien à recevoir : le
        // redemander au serveur rend 416, et l'installation échouait là.
        let dir = scratch("complet");
        let partial = dir.join("m.part");
        std::fs::write(&partial, [0u8; 10]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::Complete);
        std::fs::write(&partial, [0u8; 4]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::At(4));
        // Plus long que prévu : il ne vient pas du même fichier.
        std::fs::write(&partial, [0u8; 12]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::FromScratch);
        assert!(!partial.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_active_path_is_recognised_through_tilde_and_links() {
        let root = std::env::temp_dir().join(format!("sillon-active-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let real = root.join("home/models");
        std::fs::create_dir_all(&real).unwrap();
        let model = real.join("m.gguf");
        std::fs::write(&model, b"x").unwrap();
        let home = root.join("home");

        assert!(is_active_path_with("~/models/m.gguf", &model, Some(&home)), "forme ~");
        assert!(!is_active_path_with("~/models/autre.gguf", &model, Some(&home)));
        assert!(!is_active_path_with("", &model, Some(&home)));

        #[cfg(unix)]
        {
            let link = root.join("lien");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let via_link = link.join("m.gguf");
            assert!(is_active_path_with(&via_link.to_string_lossy(), &model, Some(&home)), "dossier lié");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn kokoro_names_its_voices_by_language() {
        assert!(voices_follow_language("kokoro"));
        assert!(!voices_follow_language("supertonic"));
        // Une française, trois anglaises : les voix retenues, rien d'autre.
        let ids = |language: &str| -> Vec<&str> {
            voices_for_family("kokoro").iter().filter(|v| v.language == language).map(|v| v.id).collect()
        };
        assert_eq!(ids("fr"), ["p-fenrir60"]);
        assert_eq!(ids("en"), ["af_bella", "am_fenrir", "af_heart"]);
        assert_eq!(voices_for_family("kokoro").len(), 4);
        assert_eq!(default_voice_for_language("kokoro", "fr"), "p-fenrir60");
        assert_eq!(default_voice_for_language("kokoro", "en-US"), "af_bella");
        assert!(voices_for_family("kokoro").iter().all(|v| v.label.contains('·')));
    }

    #[test]
    fn an_unknown_voice_gives_the_default_of_its_language() {
        assert_eq!(voice_for_language("kokoro", "en", Some("am_fenrir")), "am_fenrir");
        // Une voix retirée du catalogue retombe sur la recommandée de sa langue.
        assert_eq!(voice_for_language("kokoro", "fr", Some("ff_siwis")), "p-fenrir60");
        assert_eq!(voice_for_language("kokoro", "en", Some("bf_emma")), "af_bella");
        assert_eq!(voice_for_language("kokoro", "fr", Some("af_bella")), "p-fenrir60", "voix d'une autre langue");
        assert_eq!(voice_for_language("kokoro", "en", Some("F1")), "af_bella");
        assert_eq!(voice_for_language("kokoro", "en", None), "af_bella");
    }
}

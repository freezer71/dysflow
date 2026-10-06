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
}

impl ModelPackage {
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

/// Langues de Supertonic 3, telles que déclarées par sa spécification.
const SUPERTONIC_LANGUAGES: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr",
    "hu", "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk",
    "vi",
];

/// Langues de MagpieTTS exposées par l'intégration audio.cpp.
///
/// Le japonais du modèle amont n'y est pas : son tokeniseur n'a pas été porté.
const MAGPIE_LANGUAGES: &[&str] = &[
    "ar", "de", "en", "es", "fr", "hi", "it", "ko", "pt", "vi", "zh",
];

/// Modèles proposés à l'installation.
///
/// La variante MagpieTTS en précision d'origine n'y figure pas : sa
/// spécification embarquée nomme l'option de voix `speaker`, alors que le code
/// d'audio.cpp ne lit que `voice_id`. Le paquet se charge et parle, mais ses
/// cinq locuteurs rendent un signal identique au bit près, sans le moindre
/// message. Un modèle dont le sélecteur de voix ne fait rien n'a pas sa place
/// dans un catalogue qui promet des voix nommées.
///
/// La variante Supertonic « origine » n'y figure pas non plus : sur le dépôt,
/// `supertonic-3-orig.gguf` et `supertonic-3-q8_0.gguf` sont le même fichier,
/// à l'octet près — même taille, même empreinte. Deux entrées feraient
/// télécharger 454 Mo deux fois pour rien.
///
/// Restreint aux familles réellement compilées dans le binaire : proposer un
/// modèle que le moteur ne saurait pas charger n'aiderait personne.
pub const CATALOG: &[ModelPackage] = &[
    ModelPackage {
        id: "magpie_tts_q8_0",
        family: "magpie_tts",
        display_name: "MagpieTTS Multilingual · Q8_0",
        description: "Locuteurs nommés, cuits dans le paquet. Rien à configurer.",
        precision: "q8_0",
        languages: MAGPIE_LANGUAGES,
        repo: "audio-cpp/audio.cpp-gguf",
        revision: "main",
        file: "MagpieTTS-Multilingual-357M-GGUF/magpie-tts-multilingual-357m-q8_0.gguf",
        languages_label: "11 langues, dont le français",
        bytes: 1_562_142_912,
        recommended: true,
        pace: "Plus lent que la parole sur Apple Silicon — 1,4 fois le temps réel mesuré sur M5 : \
               la voix marque des silences entre les phrases.",
    },
    ModelPackage {
        id: "supertonic_3_f16",
        family: "supertonic",
        display_name: "Supertonic 3 · F16",
        description: "Rapide et léger. Environ 6× le temps réel sur CPU.",
        precision: "f16",
        languages: SUPERTONIC_LANGUAGES,
        repo: "audio-cpp/audio.cpp-gguf",
        revision: "main",
        file: "Supertonic-3-GGUF/supertonic-3-f16.gguf",
        languages_label: "31 langues, dont le français",
        bytes: 312_784_196,
        recommended: false,
        pace: "Vingt fois plus vite que la parole : lecture fluide, sauts immédiats.",
    },
    ModelPackage {
        id: "supertonic_3_q8_0",
        family: "supertonic",
        display_name: "Supertonic 3 · Q8_0",
        description: "Quantifié 8 bits. Plus volumineux ici, qualité équivalente.",
        precision: "q8_0",
        languages: SUPERTONIC_LANGUAGES,
        repo: "audio-cpp/audio.cpp-gguf",
        revision: "main",
        file: "Supertonic-3-GGUF/supertonic-3-q8_0.gguf",
        languages_label: "31 langues, dont le français",
        bytes: 454_072_836,
        recommended: false,
        pace: "Vingt fois plus vite que la parole : lecture fluide, sauts immédiats.",
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
/// Sans la feature `audio-cpp`, aucune famille n'est compilée : le catalogue
/// est alors vide et l'interface le dit, plutôt que de proposer des
/// installations sans effet.
pub fn available() -> Vec<&'static ModelPackage> {
    let families = compiled_families();
    if families.is_empty() {
        return Vec::new();
    }
    CATALOG
        .iter()
        .filter(|p| families.contains(&p.family))
        .collect()
}

/// Répertoire où sont rangés les modèles installés.
pub fn models_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("models"))
}

pub fn install_path(package: &ModelPackage) -> Option<PathBuf> {
    models_dir().map(|d| d.join(package.family).join(package.file_name()))
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

/// Taille sur disque si le modèle est installé.
///
/// Un téléchargement interrompu laisse un `.part`, qui n'est jamais confondu
/// avec un modèle complet.
pub fn installed_size(package: &ModelPackage) -> Option<u64> {
    let path = install_path(package)?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

pub fn is_installed(package: &ModelPackage) -> bool {
    installed_size(package).is_some()
}

/// Vrai si le fichier est là mais n'a pas la taille annoncée.
///
/// Un téléchargement interrompu ailleurs que par nous, un disque plein, une
/// copie tronquée : le fichier existe et se présente comme installé, puis le
/// moteur refuse de l'ouvrir sur un message illisible. Mieux vaut le dire.
pub fn is_truncated(package: &ModelPackage) -> bool {
    installed_size(package).is_some_and(|size| size != package.bytes)
}

/// Premier modèle installé, celui que le lecteur choisit par défaut.
pub fn first_installed() -> Option<&'static ModelPackage> {
    available().into_iter().find(|p| is_installed(p))
}

pub fn remove(package: &ModelPackage) -> Result<()> {
    let path = install_path(package).context("répertoire de données introuvable")?;
    std::fs::remove_file(&path).with_context(|| format!("suppression de {}", path.display()))?;
    // Laisse le répertoire de famille propre s'il devient vide.
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
    Ok(())
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
    /// Lance le téléchargement dans un thread dédié.
    pub fn start(package: &'static ModelPackage) -> Result<Self> {
        let destination = install_path(package).context("répertoire de données introuvable")?;
        let received = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(package.bytes));
        let outcome = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = (
            Arc::clone(&received),
            Arc::clone(&total),
            Arc::clone(&outcome),
            Arc::clone(&cancel),
        );
        let url = package.url();

        thread::spawn(move || {
            let (received, total, outcome, cancel) = worker;
            let result = fetch(&url, &destination, &received, &total, &cancel);
            let state = match result {
                Ok(()) if cancel.load(Ordering::Relaxed) => Progress::Cancelled,
                Ok(()) => Progress::Done,
                Err(err) => Progress::Failed(err.to_string()),
            };
            *outcome.lock().unwrap() = Some(state);
        });

        Ok(Self {
            package,
            received,
            total,
            outcome,
            cancel,
        })
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
            total: Arc::new(AtomicU64::new(package.bytes)),
            outcome: Arc::new(Mutex::new(finished.then_some(Progress::Done))),
            cancel: Arc::new(AtomicBool::new(false)),
        }
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
fn fetch(
    url: &str,
    destination: &Path,
    received: &AtomicU64,
    total: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("création de {}", parent.display()))?;
    }
    let partial = destination.with_extension("part");
    let expected = total.load(Ordering::Relaxed);
    let already = match resume_from(&partial, expected) {
        // Tout est déjà là : le serveur répondrait 416 à une reprise.
        Resume::Complete => {
            received.store(expected, Ordering::Relaxed);
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
    if let Some(length) = response.header("Content-Length").and_then(|v| v.parse::<u64>().ok()) {
        let full = if resuming { already + length } else { length };
        total.store(full, Ordering::Relaxed);
    }

    let mut file = if resuming {
        received.store(already, Ordering::Relaxed);
        std::fs::OpenOptions::new().append(true).open(&partial)?
    } else {
        received.store(0, Ordering::Relaxed);
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
        received.store(written, Ordering::Relaxed);
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
            return format!("{:.1} {unit}", bytes as f64 / scale as f64);
        }
    }
    format!("{bytes} o")
}

/// Une voix intégrée, proposée au choix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: &'static str,
    pub label: &'static str,
}

/// Voix de Supertonic 3 : cinq masculines, cinq féminines.
///
/// Le modèle n'accepte pas de référence externe — pas de clonage : seuls ces
/// identifiants ont un effet.
const SUPERTONIC_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "M1", label: "M1 · masculine 1" },
    VoiceOption { id: "M2", label: "M2 · masculine 2" },
    VoiceOption { id: "M3", label: "M3 · masculine 3" },
    VoiceOption { id: "M4", label: "M4 · masculine 4" },
    VoiceOption { id: "M5", label: "M5 · masculine 5" },
    VoiceOption { id: "F1", label: "F1 · féminine 1" },
    VoiceOption { id: "F2", label: "F2 · féminine 2" },
    VoiceOption { id: "F3", label: "F3 · féminine 3" },
    VoiceOption { id: "F4", label: "F4 · féminine 4" },
    VoiceOption { id: "F5", label: "F5 · féminine 5" },
];

/// Voix proposées pour une famille donnée.
///
/// Vide quand la famille n'expose pas de voix nommées : le champ redevient
/// alors libre, pour y mettre un chemin de WAV à cloner.
pub fn voices_for_family(family: &str) -> &'static [VoiceOption] {
    match family {
        "supertonic" => SUPERTONIC_VOICES,
        "magpie_tts" => MAGPIE_VOICES,
        _ => &[],
    }
}

/// Locuteurs de MagpieTTS, relevés dans le `speakers.json` embarqué du paquet.
///
/// Le modèle les accepte par nom ou par index ; le nom est plus lisible et
/// survivrait à un réordonnancement du paquet. Aucun timbre n'est décrit ici :
/// je ne les ai pas écoutés, et `t` le fait mieux qu'une étiquette.
const MAGPIE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "Aria", label: "Aria" },
    VoiceOption { id: "Jason", label: "Jason" },
    VoiceOption { id: "John", label: "John" },
    VoiceOption { id: "Leo", label: "Leo" },
    VoiceOption { id: "Sofia", label: "Sofia" },
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
        // contrôle de débit : la vitesse y est inerte. Il valide par ailleurs
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
        // Famille inconnue : on n'affirme rien, tout est présenté comme possible.
        _ => &[],
    }
}

/// Vrai si la famille reconnaît cette voix.
///
/// Une famille sans voix nommées accepte n'importe quoi — un chemin de WAV,
/// un préréglage qui lui est propre — et ne se prononce donc pas.
pub fn knows_voice(family: &str, voice: &str) -> bool {
    let voices = voices_for_family(family);
    voices.is_empty() || voice.trim().is_empty() || voices.iter().any(|v| v.id == voice.trim())
}

/// Voix par défaut d'une famille, quand elle en nomme.
pub fn default_voice(family: &str) -> &'static str {
    voices_for_family(family).first().map_or("", |v| v.id)
}

/// Écarte une voix que la famille ne reconnaît pas.
///
/// Un préréglage hérité d'un autre modèle n'a aucun sens ici : `F1` n'est pour
/// MagpieTTS ni un nom de locuteur ni un index, et le modèle le laisse
/// descendre jusqu'à `std::stoll`. L'utilisateur reçoit alors
/// « stoll: no conversion » pour toute explication.
pub fn sanitised_voice(family: &str, voice: &str) -> String {
    if knows_voice(family, voice) {
        voice.trim().to_string()
    } else {
        String::new()
    }
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
        assert_eq!(languages_for_family("supertonic").len(), 31);
        assert_eq!(languages_for_family("magpie_tts").len(), 11);
        assert!(languages_for_family("magpie_tts").contains(&"zh"));
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
            package("supertonic_3_f16").url(),
            "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/Supertonic-3-GGUF/supertonic-3-f16.gguf"
        );
        assert_eq!(
            package("magpie_tts_q8_0").url(),
            "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/MagpieTTS-Multilingual-357M-GGUF/magpie-tts-multilingual-357m-q8_0.gguf"
        );
    }

    #[test]
    fn file_name_is_extracted_from_the_repo_path() {
        assert_eq!(package("supertonic_3_f16").file_name(), "supertonic-3-f16.gguf");
        assert_eq!(package("magpie_tts_q8_0").file_name(), "magpie-tts-multilingual-357m-q8_0.gguf");
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
    fn exactly_one_package_is_recommended() {
        assert_eq!(CATALOG.iter().filter(|p| p.recommended).count(), 1);
    }

    #[test]
    fn catalog_is_empty_without_a_compiled_family() {
        // Sans la feature, aucune famille n'est compilée : rien ne doit être
        // proposé, faute de pouvoir le charger.
        if compiled_families().is_empty() {
            assert!(available().is_empty());
        } else {
            assert!(!available().is_empty());
        }
    }

    #[test]
    fn install_paths_are_scoped_by_family() {
        // Deux familles peuvent nommer leur fichier pareil : le répertoire les
        // sépare.
        let Some(path) = install_path(package("supertonic_3_f16")) else { return };
        assert!(path.ends_with("supertonic/supertonic-3-f16.gguf"), "{path:?}");
        let Some(path) = install_path(package("magpie_tts_q8_0")) else { return };
        assert!(
            path.ends_with("magpie_tts/magpie-tts-multilingual-357m-q8_0.gguf"),
            "{path:?}"
        );
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
    fn a_preset_from_another_model_is_never_forwarded() {
        // « F1 » n'est pour MagpieTTS ni un locuteur ni un index : le modèle le
        // laisse descendre jusqu'à std::stoll, qui répond « no conversion ».
        assert!(!knows_voice("magpie_tts", "F1"));
        assert_eq!(sanitised_voice("magpie_tts", "F1"), "");
        assert_eq!(default_voice("magpie_tts"), "Aria");

        // Ce que la famille connaît passe intact, vide compris.
        assert!(knows_voice("magpie_tts", "Leo"));
        assert_eq!(sanitised_voice("magpie_tts", "Leo"), "Leo");
        assert!(knows_voice("magpie_tts", ""));

        // Une famille sans voix nommées ne se prononce pas : le champ y porte
        // un chemin de WAV ou un préréglage qui lui est propre.
        assert!(knows_voice("inconnue", "n'importe quoi"));
        assert_eq!(sanitised_voice("inconnue", "/tmp/a.wav"), "/tmp/a.wav");
        assert_eq!(default_voice("inconnue"), "");
    }

    #[test]
    fn magpie_speakers_match_the_package() {
        // Relevés dans le `speakers.json` embarqué du GGUF.
        let ids: Vec<&str> = voices_for_family("magpie_tts").iter().map(|v| v.id).collect();
        assert_eq!(ids, ["Aria", "Jason", "John", "Leo", "Sofia"]);
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
        assert_eq!(human_size(2 * 1024), "2.0 Ko");
        assert_eq!(human_size(312_784_196), "298.3 Mo");
        assert_eq!(human_size(3 * (1 << 30)), "3.0 Go");
    }

    #[test]
    fn unknown_languages_are_rejected() {
        assert!(!CATALOG[0].speaks("zz"));
    }

    #[test]
    fn supertonic_exposes_its_ten_built_in_voices() {
        let voices = voices_for_family("supertonic");
        assert_eq!(voices.len(), 10);
        assert_eq!(voices[0].id, "M1");
        assert_eq!(voices[5].id, "F1");
        // Chaque voix porte un libellé lisible, pas seulement un code.
        assert!(voices.iter().all(|v| v.label.contains('·')));
    }

    #[test]
    fn unknown_families_offer_no_named_voice() {
        assert!(voices_for_family("inconnue").is_empty());
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
}

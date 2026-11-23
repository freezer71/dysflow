//! La dictée du cœur, vue de Swift : une session (`DictationSession`) par
//! dictée, au-dessus de `dysflow::stt::dictation::Dictation`, et le modèle
//! gardé d'une dictée à l'autre (`Holder`), libéré après `keep_voice_minutes`
//! sans usage comme celui de l'app.
//!
//! Les fils :
//! - `push` dépose l'audio sans jamais attendre (R41) : ni le modèle, ni une
//!   lecture, ni l'app ;
//! - le fil de la dictée (`dysflow-dictee`) charge le modèle et lit ;
//! - les nouvelles partent vers l'app par un fil à elles
//!   (`dysflow-dictee-envoi`), qui ne tient **aucun** verrou du cœur ni le
//!   verrou du GPU : l'app peut y rappeler le cœur (`cancel`, une
//!   libération) sans blocage ;
//! - `finish`, `transcribe_once` et les libérations bloquent : jamais sur le
//!   fil principal de l'app.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use dysflow::models::ModelPackage;
use dysflow::stt::detector::EnergyDetector;
use dysflow::stt::dictation::{Dictation, Engine, Event};
use dysflow::stt::holder::{idle_delay, Holder, DEFAULT_DELAY};
use dysflow::stt::recognizer::Recognizer;
use dysflow::tts::Cancel;

use crate::{failed, DysflowError};

/// Ce que la dictée dit à l'app, dans l'ordre.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum DictationUpdate {
    /// Le modèle commence à se charger (il n'était pas en mémoire).
    Loading,
    /// Le modèle est chargé, en ce temps-là.
    Loaded { seconds: f64 },
    /// Le modèle est prêt (chargé à l'instant, ou déjà là) : tout ce qui suit
    /// est de la lecture. Avant lui, un `Failed` est un chargement manqué.
    Ready,
    /// Le texte à montrer : le définitif, puis la relecture de l'énoncé en
    /// cours.
    Partial { committed: String, pending: String },
    /// Le texte définitif d'un énoncé, et son délai : de l'arrivée de l'audio
    /// qui l'a clos à ce texte.
    Committed { text: String, delay_seconds: f64 },
    /// La mémoire du modèle, en octets, après une lecture.
    Memory { bytes: u64 },
    /// La fin : tout le texte.
    Ended { text: String },
    /// Un échec, à dire. Avant `Ready`, le modèle ne s'est pas chargé et la
    /// dictée s'arrête ; après, un énoncé est perdu et la dictée continue.
    Failed { message: String },
}

impl DictationUpdate {
    /// La nouvelle d'un événement du cœur, une pour une.
    fn of(event: Event) -> Self {
        match event {
            Event::Loading => Self::Loading,
            Event::Loaded(took) => Self::Loaded {
                seconds: took.as_secs_f64(),
            },
            Event::Ready => Self::Ready,
            Event::Partial { committed, pending } => Self::Partial { committed, pending },
            Event::Committed { text, delay } => Self::Committed {
                text,
                delay_seconds: delay.as_secs_f64(),
            },
            Event::Memory(bytes) => Self::Memory { bytes },
            Event::Ended { text } => Self::Ended { text },
            Event::Failed(message) => Self::Failed { message },
        }
    }
}

/// Reçoit les nouvelles d'une dictée, fournie par l'app.
///
/// Contrat pour l'implémentation étrangère :
/// - `on_update` est appelé sur un fil du cœur réservé à cette dictée,
///   jamais le fil principal, jamais sous le verrou du GPU ni sous un verrou
///   du cœur : y rappeler le cœur, `push` ou `cancel` est permis. `finish`
///   ne l'est pas (il attendrait ce fil même) : de là, il rend une erreur
///   sans rien arrêter, et la dictée reste à finir d'ailleurs ;
/// - les appels se suivent, un à la fois, dans l'ordre de la dictée ;
/// - après `cancel`, au plus l'appel déjà en cours se termine : aucun autre ;
/// - il rend la main vite (l'app passe à son propre fil pour l'interface) :
///   les nouvelles suivantes l'attendent, jamais la dictée.
#[uniffi::export(with_foreign)]
pub trait DictationListener: Send + Sync {
    fn on_update(&self, update: DictationUpdate);
}

/// Ce qui part vers le fil d'envoi.
enum Mail {
    Update(DictationUpdate),
    /// Tout ce qui précède est envoyé : répondre.
    Flush(mpsc::SyncSender<()>),
}

/// Une dictée du cœur.
#[derive(uniffi::Object)]
pub struct DictationSession {
    dictation: Dictation,
    mail: mpsc::Sender<Mail>,
    closed: Arc<AtomicBool>,
    /// Le fil d'envoi : `finish` ne peut pas l'attendre depuis lui-même.
    delivery: std::thread::ThreadId,
}

#[uniffi::export]
impl DictationSession {
    /// Démarre une dictée par le modèle `id` (un paquet installé du
    /// catalogue), en `language` (« fr », « en »). Rend la main tout de
    /// suite : le modèle se charge sur le fil de la dictée, l'audio attend.
    /// `Err` si le paquet n'est pas installé, ou si ce binaire ne sait pas
    /// le calculer. Peut attendre la libération d'un autre modèle : hors du
    /// fil principal.
    #[uniffi::constructor]
    pub fn start(
        id: String,
        language: String,
        listener: Arc<dyn DictationListener>,
    ) -> Result<Arc<Self>, DysflowError> {
        let (dir, model) = chosen(&id)?;
        Self::begin(model, &dir, &language, listener).map(Arc::new)
    }

    /// Dépose de l'audio (16 kHz mono) ; ne bloque jamais. Après `finish` ou
    /// `cancel`, ignoré.
    pub fn push(&self, samples: Vec<f32>) {
        self.dictation.push(&samples);
    }

    /// Fin de la parole : lit tout, puis rend le texte entier. Au retour,
    /// toutes les nouvelles (`Ended` compris) ont été remises à `on_update`.
    /// **Bloque** jusqu'à la dernière lecture : jamais sur le fil principal.
    /// Appelé depuis `on_update` : une erreur, et rien n'est arrêté.
    pub fn finish(&self) -> Result<String, DysflowError> {
        if std::thread::current().id() == self.delivery {
            return Err(failed(dysflow::tr!(
                "a dictation cannot be finished from on_update: it would wait for itself",
                "une dictée ne se finit pas depuis on_update : elle s'attendrait elle-même"
            )));
        }
        let text = self.dictation.finish().map_err(failed);
        let (ack, done) = mpsc::sync_channel(1);
        if self.mail.send(Mail::Flush(ack)).is_ok() {
            let _ = done.recv();
        }
        text
    }

    /// Plus aucune nouvelle (au plus l'appel en cours se termine). Ne bloque
    /// pas : une lecture déjà partie va au bout, sans effet.
    pub fn cancel(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.dictation.cancel();
    }
}

impl DictationSession {
    fn begin(
        model: Arc<dyn KeptModel>,
        dir: &Path,
        language: &str,
        listener: Arc<dyn DictationListener>,
    ) -> Result<Self, DysflowError> {
        let (mail, inbox) = mpsc::channel::<Mail>();
        let closed = Arc::new(AtomicBool::new(false));
        let delivery_closed = closed.clone();
        // Sans fil d'envoi, rien n'arriverait à l'app : pas de dictée.
        let delivery = std::thread::Builder::new()
            .name("dysflow-dictee-envoi".into())
            .spawn(move || deliver(&inbox, &*listener, &delivery_closed))
            .map_err(|err| {
                failed(dysflow::tr!(
                    en = "the dictation won't start: {err}",
                    fr = "la dictée ne démarre pas : {err}"
                ))
            })?
            .thread()
            .id();
        let recognizer: Arc<dyn Recognizer> = model;
        let sink_mail = mail.clone();
        let dictation = Dictation::start(
            Engine::Segmented(recognizer, Box::new(EnergyDetector::new())),
            dir,
            language,
            move |event| {
                let _ = sink_mail.send(Mail::Update(DictationUpdate::of(event)));
            },
        );
        Ok(Self {
            dictation,
            mail,
            closed,
            delivery,
        })
    }
}

/// Le fil d'envoi : chaque nouvelle à l'app, sans verrou tenu.
fn deliver(inbox: &mpsc::Receiver<Mail>, listener: &dyn DictationListener, closed: &AtomicBool) {
    for mail in inbox {
        match mail {
            Mail::Update(update) => {
                if !closed.load(Ordering::SeqCst) {
                    listener.on_update(update);
                }
            }
            Mail::Flush(ack) => {
                let _ = ack.send(());
            }
        }
    }
}

/// Lit `samples` d'un bloc par le modèle `id`, chargé s'il ne l'est pas (et
/// gardé ensuite, comme pour une dictée). Des échantillons vides ne font
/// que charger : l'essai du micro s'en sert avant d'ouvrir le micro.
/// **Bloque** : jamais sur le fil principal.
#[uniffi::export]
pub fn transcribe_once(
    id: String,
    samples: Vec<f32>,
    language: String,
) -> Result<String, DysflowError> {
    let (dir, model) = chosen(&id)?;
    let _hold = model.hold();
    model.prepare(&dir, &|| {}).map_err(failed)?;
    if samples.is_empty() {
        return Ok(String::new());
    }
    model
        .transcribe(&samples, &language, &Cancel::never())
        .map(|text| text.trim().to_string())
        .map_err(failed)
}

/// Libère le modèle de dictée du cœur (le modèle choisi est supprimé, ou un
/// modèle de l'app va dicter) : ses poids et sa part du budget de MLX
/// (`"dictee"`) partent. Attend une lecture en cours : hors du fil principal.
#[uniffi::export]
pub fn core_dictation_release() {
    if let Some(kept) = kept_model() {
        kept.release();
    }
}

/// Libère le modèle du cœur s'il vient du paquet `id` (la fin d'un essai du
/// micro) ; un autre reste.
#[uniffi::export]
pub fn core_dictation_release_if_loaded(id: String) {
    let Some(model) = lock(&KEPT)
        .as_ref()
        .filter(|k| k.id == id)
        .map(|k| k.model.clone())
    else {
        return;
    };
    match installed_directory(&id) {
        Some(dir) => model.release_if_loaded(&dir),
        // Le paquet a disparu : rien d'autre ne peut venir de lui.
        None => model.release(),
    }
}

/// Le délai de libération du modèle du cœur, d'après `keep_voice_minutes`
/// (`idle_delay`), comme celui de l'app ; relu au début de chaque dictée.
#[uniffi::export]
pub fn core_dictation_set_keep_voice_minutes(minutes: i64) {
    let delay = idle_delay(minutes);
    *lock(&RELEASE_AFTER) = delay;
    if let Some(kept) = kept_model() {
        kept.set_release_after(delay);
    }
}

/// Le temps de lecture cumulé du modèle du cœur, en secondes, chargements
/// exclus : l'essai du micro en fait la différence avant et après.
#[uniffi::export]
pub fn core_dictation_compute_seconds() -> f64 {
    kept_model().map_or(0.0, |kept| kept.compute_time().as_secs_f64())
}

/// La mémoire de la copie Rust de MLX, en octets : en usage et en cache.
#[derive(uniffi::Record, Debug, Clone, PartialEq)]
pub struct CoreMlxMemory {
    pub active: u64,
    pub cache: u64,
    /// Le plafond posé : la somme des besoins inscrits (`"dictee"`,
    /// `"kokoro"`).
    pub limit: u64,
}

/// La mémoire de la copie Rust de MLX ; `None` sans elle (un binaire sans la
/// feature `mlx`), quand elle ne tient aucun modèle (rien d'inscrit au
/// budget : la lire démarrerait MLX, et fausserait une empreinte au repos),
/// ou si elle ne répond pas. Passe par le fil MLX : jamais en tenant
/// `MLXGate`.
#[uniffi::export]
pub fn core_mlx_memory() -> Option<CoreMlxMemory> {
    #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
    {
        use dysflow::compute::mlx;
        if mlx::budget::total().memory == 0 {
            return None;
        }
        let memory = mlx::memory().ok()?;
        Some(CoreMlxMemory {
            active: memory.active as u64,
            cache: memory.cache as u64,
            limit: mlx::budget::total().memory as u64,
        })
    }
    #[cfg(not(all(feature = "mlx", target_os = "macos", target_arch = "aarch64")))]
    None
}

// MARK: - Le modèle gardé

/// Un modèle gardé d'une dictée à l'autre (`Holder`).
trait KeptModel: Recognizer {
    fn set_release_after(&self, delay: Option<Duration>);
    fn release_if_loaded(&self, dir: &Path);
    fn compute_time(&self) -> Duration;
}

impl<R: Recognizer + 'static> KeptModel for Holder<R> {
    fn set_release_after(&self, delay: Option<Duration>) {
        Holder::set_release_after(self, delay);
    }
    fn release_if_loaded(&self, dir: &Path) {
        Holder::release_if_loaded(self, dir);
    }
    fn compute_time(&self) -> Duration {
        Holder::compute_time(self)
    }
}

struct Kept {
    id: String,
    model: Arc<dyn KeptModel>,
}

/// Le modèle de dictée du cœur : un seul à la fois.
static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
/// Le délai de libération des modèles du cœur.
static RELEASE_AFTER: Mutex<Option<Duration>> = Mutex::new(Some(DEFAULT_DELAY));

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn kept_model() -> Option<Arc<dyn KeptModel>> {
    lock(&KEPT).as_ref().map(|k| k.model.clone())
}

/// Le dossier installé du paquet `id`, et son modèle gardé.
fn chosen(id: &str) -> Result<(PathBuf, Arc<dyn KeptModel>), DysflowError> {
    let package = dysflow::models::CATALOG
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| {
            failed(dysflow::tr!(
                en = "unknown dictation model: {id}",
                fr = "modèle de dictée inconnu : {id}"
            ))
        })?;
    let dir = installed_directory(id).ok_or_else(|| {
        failed(dysflow::tr!(
            en = "{} isn't installed",
            fr = "{} n'est pas installé",
            package.display_name.get()
        ))
    })?;
    let model = kept(id, || model_for(package))?;
    Ok((dir, model))
}

/// Le modèle gardé pour `id` ; un autre gardé est d'abord libéré (jamais
/// deux en mémoire, ni deux parts de budget).
fn kept(
    id: &str,
    make: impl FnOnce() -> Result<Arc<dyn KeptModel>, DysflowError>,
) -> Result<Arc<dyn KeptModel>, DysflowError> {
    let mut slot = lock(&KEPT);
    if let Some(kept) = slot.as_ref().filter(|k| k.id == id) {
        return Ok(kept.model.clone());
    }
    let model = make()?;
    let old = slot.replace(Kept {
        id: id.to_string(),
        model: model.clone(),
    });
    drop(slot);
    if let Some(old) = old {
        old.model.release();
    }
    Ok(model)
}

/// Le reconnaisseur du cœur pour ce paquet (`stt::recognizer_for`, la seule
/// liste des moteurs), gardé par un `Holder` d'une dictée à l'autre.
fn model_for(package: &ModelPackage) -> Result<Arc<dyn KeptModel>, DysflowError> {
    let recognizer = dysflow::stt::recognizer_for(package, threads()).map_err(failed)?;
    Ok(Arc::new(Holder::new(recognizer, *lock(&RELEASE_AFTER))))
}

/// Les fils de calcul d'un moteur qui en prend (sherpa) ; MLX les ignore.
fn threads() -> u32 {
    std::thread::available_parallelism().map_or(4, |n| n.get().min(8) as u32)
}

/// Vrai si le cœur sait dicter ce paquet du catalogue dans ce binaire. La
/// bascule de l'app (`CoreDictation.ported`) n'y met que de tels paquets :
/// un essai le vérifie.
#[uniffi::export]
pub fn core_dictation_supports(id: String) -> bool {
    dysflow::models::CATALOG
        .iter()
        .find(|p| p.id == id)
        .is_some_and(|package| dysflow::stt::recognizer_for(package, 1).is_ok())
}

/// Le dossier d'un paquet installé, comme `DysflowReader::model_directory`.
pub(crate) fn installed_directory(id: &str) -> Option<PathBuf> {
    let package = dysflow::models::available_with(true)
        .into_iter()
        .find(|p| p.id == id)?;
    dysflow::models::is_installed(package)
        .then(|| dysflow::models::install_path(package))
        .flatten()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

#[cfg(test)]
mod tests;

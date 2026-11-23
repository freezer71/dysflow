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
use dysflow::stt::recognizer::{Hold, Recognizer};
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
    /// La nouvelle d'un événement du cœur. `Loaded` est suivi de `Ready`
    /// (voir `Announced`).
    fn of(event: Event) -> Vec<Self> {
        match event {
            Event::Loading => vec![Self::Loading],
            Event::Loaded(took) => vec![
                Self::Loaded {
                    seconds: took.as_secs_f64(),
                },
                Self::Ready,
            ],
            Event::Partial { committed, pending } => vec![Self::Partial { committed, pending }],
            Event::Committed { text, delay } => vec![Self::Committed {
                text,
                delay_seconds: delay.as_secs_f64(),
            }],
            Event::Memory(bytes) => vec![Self::Memory { bytes }],
            Event::Ended { text } => vec![Self::Ended { text }],
            Event::Failed(message) => vec![Self::Failed { message }],
        }
    }
}

/// Reçoit les nouvelles d'une dictée, fournie par l'app.
///
/// Contrat pour l'implémentation étrangère :
/// - `on_update` est appelé sur un fil du cœur réservé à cette dictée,
///   jamais le fil principal, jamais sous le verrou du GPU ni sous un verrou
///   du cœur : y rappeler la session ou le cœur est permis ;
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
    pub fn finish(&self) -> Result<String, DysflowError> {
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
        std::thread::Builder::new()
            .name("dysflow-dictee-envoi".into())
            .spawn(move || deliver(&inbox, &*listener, &delivery_closed))
            .map_err(|err| {
                failed(dysflow::tr!(
                    en = "the dictation won't start: {err}",
                    fr = "la dictée ne démarre pas : {err}"
                ))
            })?;
        let recognizer: Arc<dyn Recognizer> = Arc::new(Announced {
            inner: model,
            mail: mail.clone(),
        });
        let sink_mail = mail.clone();
        let dictation = Dictation::start(
            Engine::Segmented(recognizer, Box::new(EnergyDetector::new())),
            dir,
            language,
            move |event| {
                for update in DictationUpdate::of(event) {
                    let _ = sink_mail.send(Mail::Update(update));
                }
            },
        );
        Ok(Self {
            dictation,
            mail,
            closed,
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

/// Le modèle gardé, vu par une dictée : il dit `Ready` quand la préparation
/// a réussi sans charger (le modèle était là). Un vrai chargement le dit par
/// `Event::Loaded`, que la dictée émet juste après (`DictationUpdate::of`).
struct Announced {
    inner: Arc<dyn KeptModel>,
    mail: mpsc::Sender<Mail>,
}

impl Recognizer for Announced {
    fn load(&self, model_dir: &Path) -> anyhow::Result<()> {
        self.inner.load(model_dir)
    }
    fn warm_up(&self) -> anyhow::Result<()> {
        self.inner.warm_up()
    }
    fn transcribe(
        &self,
        samples: &[f32],
        language: &str,
        cancel: &Cancel,
    ) -> anyhow::Result<String> {
        self.inner.transcribe(samples, language, cancel)
    }
    fn release(&self) {
        self.inner.release();
    }
    fn prepare(&self, model_dir: &Path, will_load: &dyn Fn()) -> anyhow::Result<Option<Duration>> {
        let prepared = self.inner.prepare(model_dir, will_load);
        if let Ok(None) = prepared {
            let _ = self.mail.send(Mail::Update(DictationUpdate::Ready));
        }
        prepared
    }
    fn hold(&self) -> Hold {
        self.inner.hold()
    }
    fn memory(&self) -> Option<u64> {
        self.inner.memory()
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

/// Le reconnaisseur du cœur pour ce paquet, gardé par un `Holder`. La
/// liste des moteurs que l'app fait dicter par le cœur : celle de ce
/// binaire (Parakeet MLX avec la feature `mlx`).
fn model_for(package: &ModelPackage) -> Result<Arc<dyn KeptModel>, DysflowError> {
    let release_after = *lock(&RELEASE_AFTER);
    match package.runtime {
        #[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
        dysflow::models::Runtime::ParakeetMlx => Ok(Arc::new(Holder::new(
            dysflow::stt::mlx::parakeet::ParakeetMlx::new(),
            release_after,
        ))),
        runtime => {
            let _ = release_after;
            Err(failed(dysflow::tr!(
                en = "this build cannot dictate with {} (engine {runtime:?})",
                fr = "ce binaire ne sait pas dicter avec {} (moteur {runtime:?})",
                package.display_name.get()
            )))
        }
    }
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

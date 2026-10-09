//! Liaison Rust ↔ audio.cpp, via le shim C de `native/`.
//!
//! Le modèle est chargé une fois ; chaque thread de synthèse ouvre ensuite sa
//! propre session. Cette répartition n'est pas arbitraire : côté C++, les poids
//! sont détenus par un `shared_ptr<const Assets>` que `create_task_session()`
//! — méthode `const` — se contente de partager, tandis que l'état de graphe
//! reste propre à la session. Plusieurs synthèses peuvent donc s'exécuter en
//! parallèle sur un seul chargement en mémoire.

use anyhow::{anyhow, Result};
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use super::{EngineSettings, PcmBuffer, TtsEngine, TtsParams, TtsSession};

#[repr(C)]
struct AcppAudio {
    samples: *mut c_float,
    sample_count: usize,
    sample_rate: c_int,
    channels: c_int,
}

#[repr(C)]
struct AcppParams {
    voice: *const c_char,
    language: *const c_char,
    emotion: *const c_char,
    speaking_rate: c_float,
    pitch_shift: c_float,
    energy: c_float,
    option_keys: *const *const c_char,
    option_values: *const *const c_char,
    option_count: usize,
}

extern "C" {
    fn acpp_engine_load(
        model_path: *const c_char,
        family: *const c_char,
        backend: *const c_char,
        threads: c_int,
        out_error: *mut *mut c_char,
    ) -> *mut c_void;
    fn acpp_engine_free(engine: *mut c_void);
    fn acpp_engine_family(engine: *const c_void) -> *mut c_char;
    fn acpp_engine_requires_reference(engine: *const c_void) -> c_int;
    fn acpp_session_open(engine: *const c_void, out_error: *mut *mut c_char) -> *mut c_void;
    fn acpp_session_free(session: *mut c_void);
    fn acpp_session_synthesize(
        session: *mut c_void,
        text: *const c_char,
        params: *const AcppParams,
        out_audio: *mut AcppAudio,
        out_error: *mut *mut c_char,
    ) -> c_int;
    fn acpp_audio_free(audio: *mut AcppAudio);
    fn acpp_string_free(text: *mut c_char);
}

/// Récupère et libère un message d'erreur produit par le shim.
///
/// # Safety
/// `slot` doit avoir été renseigné par une fonction du shim, ou valoir nul.
unsafe fn take_error(slot: *mut c_char, fallback: &str) -> String {
    if slot.is_null() {
        return fallback.to_string();
    }
    let message = CStr::from_ptr(slot).to_string_lossy().into_owned();
    acpp_string_free(slot);
    message
}

/// Sérialise tout ce qui charge des poids partagés.
///
/// audio.cpp partage les poids entre sessions derrière un
/// `shared_ptr<const Assets>`, mais ce `const` ne rend rien thread-safe : le
/// lecteur de tenseurs porte un cache rempli paresseusement
/// (`mutable BinaryBlob bytes_`, `tensor_source.cpp:663`), et le paquet de
/// ressources un cache de sources tout aussi `mutable`. Deux sessions qui
/// téléversent leurs poids en même temps mutent donc le même cache sans
/// verrou : l'une réalloue pendant que l'autre lit, et le `memmove` de
/// `BackendWeightStore::upload` part hors de la mémoire cartographiée.
///
/// Mesuré : sans verrou, trois plantages sur vingt fermetures ; avec un
/// `RwLock` laissant les ouvertures concurrentes, six sur vingt-cinq ; avec ce
/// verrou exclusif, aucun sur quatre-vingt-douze. Les ouvertures doivent donc
/// bien se succéder, ce qui coûte d'autant plus cher que la famille est lente
/// à ouvrir ses sessions.
///
/// L'ouverture ne suffit pas : certaines familles ne chargent leurs composants
/// qu'à la première synthèse — encodeur de locuteur d'un côté, vocodeur de
/// l'autre — et deux sessions qui démarrent ensemble se retrouvent alors dans
/// le même cache. La **première** synthèse de chaque session prend donc le
/// verrou elle aussi. Les suivantes ne le prennent plus : chaque session a son
/// propre graphe, et le parallélisme reprend ses droits.
static LIFECYCLE: Mutex<()> = Mutex::new(());

/// Prend le verrou de cycle de vie, même si un thread a paniqué en le tenant.
///
/// Le verrou ne protège aucun invariant Rust : rien n'est empoisonné pour de
/// bon, et renoncer à ouvrir ou à libérer serait pire que de continuer.
fn lifecycle() -> MutexGuard<'static, ()> {
    LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct AudioCppEngine {
    handle: *mut c_void,
    family: String,
    sessions: usize,
    /// Vrai quand la famille ne parle qu'en clonant un WAV de référence.
    requires_reference: bool,
}

// Le moteur ne fait que partager des poids immuables entre sessions ; aucune
// mutation n'a lieu après le chargement.
unsafe impl Send for AudioCppEngine {}
unsafe impl Sync for AudioCppEngine {}

impl AudioCppEngine {
    pub fn load(settings: &EngineSettings) -> Result<Self> {
        if settings.model_path.trim().is_empty() {
            return Err(anyhow!(
                "aucun modèle configuré — renseignez engine.model_path ou --model"
            ));
        }

        let model = CString::new(settings.model_path.as_str())?;
        let family = CString::new(settings.family.as_str())?;
        let backend = CString::new(settings.backend.as_str())?;
        let mut error: *mut c_char = ptr::null_mut();

        let _guard = lifecycle();
        // SAFETY : les chaînes vivent jusqu'au retour de l'appel, et le shim
        // capture toute exception avant de rendre la main.
        let handle = unsafe {
            acpp_engine_load(
                model.as_ptr(),
                family.as_ptr(),
                backend.as_ptr(),
                settings.threads as c_int,
                &mut error,
            )
        };
        if handle.is_null() {
            return Err(anyhow!(unsafe {
                take_error(error, "chargement du modèle impossible")
            }));
        }

        // SAFETY : `handle` est non nul et provient du shim.
        let family = unsafe {
            let raw = acpp_engine_family(handle);
            if raw.is_null() {
                "inconnu".to_string()
            } else {
                let name = CStr::from_ptr(raw).to_string_lossy().into_owned();
                acpp_string_free(raw);
                name
            }
        };

        // SAFETY : `handle` est non nul et provient du shim.
        let requires_reference = unsafe { acpp_engine_requires_reference(handle) } == 1;

        Ok(Self {
            handle,
            family,
            sessions: settings.sessions.max(1) as usize,
            requires_reference,
        })
    }
}

impl Drop for AudioCppEngine {
    fn drop(&mut self) {
        let _guard = lifecycle();
        // SAFETY : `handle` provient d'`acpp_engine_load` et n'est libéré qu'ici.
        unsafe { acpp_engine_free(self.handle) };
    }
}

impl TtsEngine for AudioCppEngine {
    fn name(&self) -> String {
        format!("audio.cpp · {}", self.family)
    }

    fn open_session(&self) -> Result<Box<dyn TtsSession>> {
        let mut error: *mut c_char = ptr::null_mut();
        let _guard = lifecycle();
        // SAFETY : le moteur est vivant tant que cette référence l'est.
        let handle = unsafe { acpp_session_open(self.handle, &mut error) };
        if handle.is_null() {
            return Err(anyhow!(unsafe {
                take_error(error, "ouverture de session impossible")
            }));
        }
        Ok(Box::new(AudioCppSessionHandle {
            handle,
            family: self.family.clone(),
            warm: false,
            rejected: std::collections::HashSet::new(),
            voice_key: None,
        }))
    }

    fn max_sessions(&self) -> usize {
        self.sessions
    }

    fn requires_reference(&self) -> bool {
        self.requires_reference
    }
}

struct AudioCppSessionHandle {
    handle: *mut c_void,
    /// Famille chargée, pour ne transmettre que les options qu'elle accepte.
    family: String,
    /// Faux tant que la session n'a pas fait sa première synthèse.
    warm: bool,
    /// Options que ce paquet a explicitement refusées.
    ///
    /// La liste des options acceptées est celle de la spécification embarquée
    /// dans le paquet, et non celle de la famille : deux variantes du même
    /// modèle ne l'écrivent pas pareil. Plutôt que de la deviner, on retient ce
    /// que le modèle a refusé et on n'y revient pas.
    rejected: std::collections::HashSet<String>,
    /// Orthographe retenue pour l'option qui porte la voix, si l'une a pris.
    voice_key: Option<usize>,
}

// Une session est confiée à un seul thread de synthèse ; elle circule entre
// threads à la création, mais n'est jamais partagée.
unsafe impl Send for AudioCppSessionHandle {}

impl Drop for AudioCppSessionHandle {
    fn drop(&mut self) {
        let _guard = lifecycle();
        // SAFETY : `handle` provient d'`acpp_session_open`.
        unsafe { acpp_session_free(self.handle) };
    }
}

impl TtsSession for AudioCppSessionHandle {
    fn synthesize(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        // Quatre tentatives suffisent : chacune écarte définitivement une
        // option, et aucune famille n'en refuse davantage.
        for _ in 0..4 {
            let outcome = if !self.warm {
                // Première synthèse : elle déclenche le chargement paresseux
                // des composants, qui puisent dans le cache de poids partagé.
                // Le marquage précède l'appel — réessayer sous verrou après un
                // échec n'apporterait rien, et laisserait le verrou pris à
                // chaque phrase.
                self.warm = true;
                let _guard = lifecycle();
                self.call(text, params)
            } else {
                self.call(text, params)
            };

            let Err(err) = outcome else {
                return outcome;
            };
            // Le modèle nomme l'option fautive : on la retire et on réessaie,
            // au lieu de renvoyer un message que personne ne peut corriger.
            match rejected_option(&err.to_string()) {
                Some(key) if self.retire(&key) => continue,
                _ => return Err(err),
            }
        }
        self.call(text, params)
    }
}

/// Compose les options à transmettre, voix comprise.
///
/// La voix n'est posée que sous l'orthographe encore en lice, et jamais sous
/// une orthographe déjà refusée : y laisser les deux ferait échouer la requête
/// sur celle que le paquet ne connaît pas.
fn deliverable_options(
    base: std::collections::BTreeMap<String, String>,
    candidates: &[&str],
    voice_key: Option<usize>,
    voice: &str,
    rejected: &std::collections::HashSet<String>,
) -> std::collections::BTreeMap<String, String> {
    let mut options = base;
    for key in candidates {
        options.remove(*key);
    }
    if !voice.is_empty() {
        if let Some(key) = candidates.get(voice_key.unwrap_or(0)) {
            options.insert((*key).to_string(), voice.to_string());
        }
    }
    options.retain(|key, _| !rejected.contains(key));
    options
}

/// Nom de l'option qu'un modèle vient de refuser, s'il l'a nommée.
///
/// Le message a la forme « unknown MagpieTTS request option: voice_id ».
fn rejected_option(message: &str) -> Option<String> {
    let (before, key) = message.rsplit_once("request option: ")?;
    if !before.contains("unknown") {
        return None;
    }
    let key = key.trim().trim_end_matches('.').to_string();
    (!key.is_empty()).then_some(key)
}

impl AudioCppSessionHandle {
    /// Écarte une option refusée. Faux si elle l'était déjà.
    ///
    /// Quand c'est l'orthographe de la voix qui est refusée, on passe à la
    /// suivante plutôt que d'abandonner : renoncer ferait parler tous les
    /// locuteurs de la même voix, en silence.
    fn retire(&mut self, key: &str) -> bool {
        let candidates = crate::models::voice_option_keys(&self.family);
        if let Some(position) = candidates.iter().position(|c| *c == key) {
            let next = position + 1;
            if next < candidates.len() {
                self.voice_key = Some(next);
                return true;
            }
            self.voice_key = None;
        }
        self.rejected.insert(key.to_string())
    }

    /// Options réellement transmises, une fois écarté ce que le paquet refuse.
    fn options_for(&self, params: &TtsParams) -> std::collections::BTreeMap<String, String> {
        deliverable_options(
            params.model_options(&self.family),
            crate::models::voice_option_keys(&self.family),
            self.voice_key,
            &params.voice,
            &self.rejected,
        )
    }

    fn call(&mut self, text: &str, params: &TtsParams) -> Result<PcmBuffer> {
        let text_c = CString::new(text)?;
        let voice = CString::new(params.voice.as_str())?;
        let language = CString::new(params.language.as_str())?;
        let emotion = CString::new(params.emotion.as_str())?;

        // Les options doivent survivre à l'appel : on garde les `CString`
        // vivantes dans ce cadre, et on ne transmet que leurs pointeurs.
        let options = self.options_for(params);
        let mut keys = Vec::with_capacity(options.len());
        let mut values = Vec::with_capacity(options.len());
        for (key, value) in &options {
            keys.push(CString::new(key.as_str())?);
            values.push(CString::new(value.as_str())?);
        }
        let key_ptrs: Vec<*const c_char> = keys.iter().map(|k| k.as_ptr()).collect();
        let value_ptrs: Vec<*const c_char> = values.iter().map(|v| v.as_ptr()).collect();

        let ffi_params = AcppParams {
            voice: voice.as_ptr(),
            language: language.as_ptr(),
            emotion: emotion.as_ptr(),
            // Toujours 1× : la vitesse s'applique au son, à la lecture.
            speaking_rate: 1.0,
            pitch_shift: params.pitch_shift,
            energy: params.energy,
            option_keys: key_ptrs.as_ptr(),
            option_values: value_ptrs.as_ptr(),
            option_count: key_ptrs.len(),
        };

        let mut audio = AcppAudio {
            samples: ptr::null_mut(),
            sample_count: 0,
            sample_rate: 0,
            channels: 0,
        };
        let mut error: *mut c_char = ptr::null_mut();

        // SAFETY : toutes les chaînes et tous les tableaux référencés restent
        // vivants jusqu'au retour ; le shim ne conserve aucun pointeur.
        let status = unsafe {
            acpp_session_synthesize(
                self.handle,
                text_c.as_ptr(),
                &ffi_params,
                &mut audio,
                &mut error,
            )
        };

        if status != 0 {
            return Err(anyhow!(unsafe { take_error(error, "synthèse échouée") }));
        }
        if audio.samples.is_null() || audio.sample_count == 0 {
            // SAFETY : libérer un tampon vide reste correct.
            unsafe { acpp_audio_free(&mut audio) };
            return Err(anyhow!("le modèle n'a produit aucun échantillon"));
        }

        // SAFETY : le shim garantit `sample_count` flottants valides.
        let samples =
            unsafe { std::slice::from_raw_parts(audio.samples, audio.sample_count) }.to_vec();
        let sample_rate = audio.sample_rate.max(1) as u32;
        let channels = audio.channels.max(1) as u16;
        // SAFETY : le tampon appartient au shim, qui seul sait le libérer.
        unsafe { acpp_audio_free(&mut audio) };

        Ok(PcmBuffer::new(samples, sample_rate, channels))
    }
}

#[cfg(test)]
mod tests {
    use super::{deliverable_options, rejected_option};
    use std::collections::{BTreeMap, HashSet};

    fn base() -> BTreeMap<String, String> {
        BTreeMap::from([("seed".to_string(), "42".to_string())])
    }

    #[test]
    fn the_voice_uses_the_spelling_still_in_the_running() {
        let candidates = ["voice_id", "speaker"];
        let rejected = HashSet::new();

        // Premier essai : la première orthographe.
        let out = deliverable_options(base(), &candidates, None, "Sofia", &rejected);
        assert_eq!(out.get("voice_id").map(String::as_str), Some("Sofia"));
        assert!(!out.contains_key("speaker"), "les deux ne doivent jamais partir ensemble");

        // Après un refus, la suivante — et seulement elle.
        let out = deliverable_options(base(), &candidates, Some(1), "Sofia", &rejected);
        assert_eq!(out.get("speaker").map(String::as_str), Some("Sofia"));
        assert!(!out.contains_key("voice_id"));

        // Sans voix choisie, aucune des deux.
        let out = deliverable_options(base(), &candidates, None, "", &rejected);
        assert!(!out.contains_key("voice_id") && !out.contains_key("speaker"));
        // Le reste passe intact.
        assert_eq!(out.get("seed").map(String::as_str), Some("42"));
    }

    #[test]
    fn a_refused_option_never_comes_back() {
        let rejected = HashSet::from(["seed".to_string()]);
        let out = deliverable_options(base(), &[], None, "", &rejected);
        assert!(out.is_empty());
    }

    #[test]
    fn a_refused_option_is_named_and_retired() {
        assert_eq!(
            rejected_option("unknown MagpieTTS request option: voice_id").as_deref(),
            Some("voice_id")
        );
        assert_eq!(
            rejected_option("unknown Fun-ASR-Nano request option: beam.").as_deref(),
            Some("beam")
        );
    }

    #[test]
    fn other_failures_are_not_mistaken_for_a_refused_option() {
        // Sans quoi une vraie panne serait réessayée trois fois avant d'être
        // signalée, et son message remplacé par un autre.
        assert!(rejected_option("le modèle n'a produit aucun audio").is_none());
        assert!(rejected_option("MagpieTTS voice_id must be a baked voice index").is_none());
        assert!(rejected_option("request option: sans le mot attendu").is_none());
    }
}

//! Aperçu de voix : entendre un réglage avant de l'adopter.
//!
//! Régler une voix sans jamais l'entendre revient à choisir à l'aveugle : les
//! identifiants `M1`…`F5` ne disent rien du timbre, et un débit ou une
//! intonation ne se jugent qu'à l'oreille. Une phrase de démonstration est donc
//! synthétisée à la demande, avec exactement les paramètres courants, puis jouée
//! à côté de la lecture — sans toucher à sa file ni à sa position.
//!
//! La synthèse se fait dans un thread dédié qui garde sa session ouverte : le
//! premier aperçu paie l'ouverture, les suivants ne paient que la synthèse.
//! Chaque demande porte un numéro de génération, si bien qu'enchaîner les
//! essais ne fait jamais entendre un réglage abandonné entre-temps.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{PcmBuffer, TtsEngine, TtsParams};

/// Phrase de démonstration, dans la langue configurée.
///
/// Chacune porte une virgule : une suite de mots isolés ne laisserait rien
/// entendre du phrasé, qui est justement ce qu'on juge. Chacune reste courte —
/// cinq secondes — parce qu'on enchaîne dix voix pour les comparer, et qu'un
/// extrait de dix secondes rendrait la comparaison pénible.
pub fn sample_text(language: &str) -> &'static str {
    let code = crate::text::language::Lang::parse(language).map_or_else(
        || language.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase(),
        |l| l.code().to_string(),
    );
    match code.as_str() {
        "en" => "Hello, this is a preview of the selected voice, at the current speed.",
        "de" => "Guten Tag, dies ist eine Hörprobe der gewählten Stimme, in diesem Tempo.",
        "es" => "Hola, esta es una muestra de la voz seleccionada, a esta velocidad.",
        "it" => "Buongiorno, questa è un'anteprima della voce selezionata, a questa velocità.",
        "pt" => "Olá, esta é uma amostra da voz selecionada, a esta velocidade.",
        "nl" => "Goedendag, dit is een voorbeeld van de gekozen stem, op deze snelheid.",
        _ => "Bonjour, voici un aperçu de la voix sélectionnée, à cette vitesse.",
    }
}

/// Refus d'un aperçu sans sortie audio.
pub const AUDIO_MISSING: &str = "sortie audio indisponible : aucun aperçu possible";

/// Lance l'aperçu des réglages en cours sur la phrase d'exemple : le
/// précédent est coupé, un refus s'explique, la lecture se met en pause pour
/// que l'aperçu parle seul. Chemin unique de l'app (avec ou sans texte).
pub fn start(
    previewer: &mut Previewer,
    player: &mut crate::audio::Player,
    config: &crate::config::Config,
    language: &str,
    label: String,
) -> Result<(), String> {
    previewer.cancel();
    player.stop_preview();
    if !player.is_available() {
        return Err(AUDIO_MISSING.to_string());
    }
    player.pause();
    previewer
        .request(sample_text(language).to_string(), config.tts_params_in(language), label)
        .map_err(|err| format!("aperçu impossible : {err}"))
}

struct Request {
    generation: u64,
    text: String,
    params: TtsParams,
}

/// Aperçu terminé, rattaché à la demande qui l'a produit.
struct Outcome {
    generation: u64,
    audio: Result<PcmBuffer, String>,
}

/// Fabrique d'aperçus, adossée à un moteur.
pub struct Previewer {
    requests: Option<Sender<Request>>,
    results: Receiver<Outcome>,
    worker: Option<JoinHandle<()>>,
    generation: u64,
    /// Ce que le dernier aperçu demandé fait entendre.
    label: String,
    /// Renseigné tant que la synthèse tourne.
    started: Option<Instant>,
}

impl Previewer {
    /// Prépare les aperçus sans rien charger.
    ///
    /// Le moteur n'est sollicité qu'à la première demande : ouvrir une session
    /// au démarrage rendrait au lancement les secondes qu'on vient de lui
    /// retirer.
    pub fn new(engine: Arc<dyn TtsEngine>) -> Self {
        let (requests, inbox) = mpsc::channel();
        let (outbox, results) = mpsc::channel();
        let worker = crate::sync::spawn_named("sillon-apercu", move || worker(engine, inbox, outbox));
        Self {
            requests: Some(requests),
            results,
            worker: Some(worker),
            generation: 0,
            label: String::new(),
            started: None,
        }
    }

    /// Demande un aperçu, en périmant celui qui serait encore en route.
    pub fn request(&mut self, text: String, params: TtsParams, label: String) -> Result<(), String> {
        self.generation += 1;
        self.requests
            .as_ref()
            .ok_or_else(|| "le moteur d'aperçu s'est arrêté".to_string())?
            .send(Request {
                generation: self.generation,
                text,
                params,
            })
            .map_err(|_| "le moteur d'aperçu s'est arrêté".to_string())?;
        self.label = label;
        self.started = Some(Instant::now());
        Ok(())
    }

    /// Ce que l'aperçu courant fait entendre.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Depuis combien de temps la synthèse tourne, si elle tourne.
    pub fn pending_for(&self) -> Option<Duration> {
        self.started.map(|t| t.elapsed())
    }

    /// Récupère un aperçu prêt, en jetant ceux qu'une demande plus récente a
    /// périmés.
    pub fn poll(&mut self) -> Option<Result<PcmBuffer, String>> {
        loop {
            match self.results.try_recv() {
                Ok(outcome) if outcome.generation == self.generation => {
                    self.started = None;
                    return Some(outcome.audio);
                }
                // Réglage abandonné entre-temps : on ne le fait pas entendre.
                Ok(_) => continue,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    self.started = None;
                    return None;
                }
            }
        }
    }

    /// Abandonne l'aperçu en cours.
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.started = None;
    }
}

impl Drop for Previewer {
    /// Attend la fin du thread pour que la session soit détruite ici.
    ///
    /// La laisser filer avec le processus qui se termine fait échouer un
    /// `GGML_ASSERT` dans le destructeur du périphérique Metal : les tampons
    /// d'une session encore vivante n'ont jamais été rendus.
    fn drop(&mut self) {
        self.requests = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker(engine: Arc<dyn TtsEngine>, requests: Receiver<Request>, results: Sender<Outcome>) {
    let mut session = None;

    while let Ok(request) = requests.recv() {
        // Si l'utilisateur a déjà changé d'avis, seule la dernière demande
        // mérite d'être synthétisée.
        let request = latest(request, &requests);

        if session.is_none() {
            match engine.open_session() {
                Ok(opened) => session = Some(opened),
                Err(err) => {
                    let outcome = Outcome {
                        generation: request.generation,
                        audio: Err(format!("ouverture de session : {err}")),
                    };
                    if results.send(outcome).is_err() {
                        return;
                    }
                    continue;
                }
            }
        }

        let audio = session
            .as_mut()
            .expect("session ouverte juste au-dessus")
            .synthesize(&request.text, &request.params)
            .map_err(|e| e.to_string())
            .and_then(|pcm| {
                if pcm.is_empty() {
                    Err("aucun échantillon produit".to_string())
                } else {
                    Ok(pcm)
                }
            });

        let outcome = Outcome {
            generation: request.generation,
            audio,
        };
        if results.send(outcome).is_err() {
            return;
        }
    }
}

/// Retient la demande la plus récente parmi celles en attente.
fn latest(mut request: Request, requests: &Receiver<Request>) -> Request {
    while let Ok(newer) = requests.try_recv() {
        request = newer;
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn a_regional_english_code_gets_the_english_sample() {
        assert_eq!(sample_text("en-US"), sample_text("en"));
        assert_ne!(sample_text("en_GB"), sample_text("fr"));
    }

    use anyhow::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Moteur d'essai : compte les synthèses et rend un signal reconnaissable.
    struct CountingEngine {
        calls: Arc<AtomicUsize>,
    }

    struct CountingSession {
        calls: Arc<AtomicUsize>,
    }

    impl TtsEngine for CountingEngine {
        fn name(&self) -> String {
            "essai".to_string()
        }
        fn open_session(&self) -> Result<Box<dyn super::super::TtsSession>> {
            Ok(Box::new(CountingSession {
                calls: Arc::clone(&self.calls),
            }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl super::super::TtsSession for CountingSession {
        fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(PcmBuffer::new(vec![0.25; text.len().max(1)], 16_000, 1))
        }
    }

    fn wait_for(previewer: &mut Previewer) -> Result<PcmBuffer, String> {
        for _ in 0..500 {
            if let Some(outcome) = previewer.poll() {
                return outcome;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("aucun aperçu en cinq secondes");
    }

    #[test]
    fn a_request_comes_back_as_audio() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(CountingEngine {
            calls: Arc::clone(&calls),
        });
        let mut previewer = Previewer::new(engine);

        assert!(previewer.pending_for().is_none());
        previewer
            .request("Bonjour".into(), TtsParams::default(), "M1 · 1,00×".into())
            .unwrap();
        assert!(previewer.pending_for().is_some());
        assert_eq!(previewer.label(), "M1 · 1,00×");

        let audio = wait_for(&mut previewer).expect("synthèse réussie");
        assert!(!audio.is_empty());
        // La demande est retombée : plus rien n'est en attente.
        assert!(previewer.pending_for().is_none());
    }

    #[test]
    fn only_the_last_setting_is_heard() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(CountingEngine {
            calls: Arc::clone(&calls),
        });
        let mut previewer = Previewer::new(engine);

        for voice in ["M1", "M2", "M3", "F1"] {
            let params = TtsParams {
                voice: voice.to_string(),
                ..TtsParams::default()
            };
            previewer
                .request("Bonjour".into(), params, voice.to_string())
                .unwrap();
        }
        wait_for(&mut previewer).expect("synthèse réussie");
        assert_eq!(previewer.label(), "F1");

        // Les demandes périmées ne ressortent jamais.
        thread::sleep(Duration::from_millis(50));
        assert!(previewer.poll().is_none());
    }

    #[test]
    fn a_cancelled_preview_is_never_played() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(CountingEngine {
            calls: Arc::clone(&calls),
        });
        let mut previewer = Previewer::new(engine);

        previewer
            .request("Bonjour".into(), TtsParams::default(), "M1".into())
            .unwrap();
        previewer.cancel();
        assert!(previewer.pending_for().is_none());

        thread::sleep(Duration::from_millis(100));
        assert!(previewer.poll().is_none());
    }

    #[test]
    fn every_configurable_language_has_its_own_sample() {
        let mut seen = std::collections::HashSet::new();
        for language in ["fr", "en", "de", "es", "it", "pt", "nl"] {
            let text = sample_text(language);
            assert!(text.contains(','), "{language} : pas de phrasé à juger");
            assert!(seen.insert(text), "{language} : échantillon dupliqué");
        }
        // Une langue inconnue retombe sur le français plutôt que sur du vide.
        assert_eq!(sample_text("xx"), sample_text("fr"));
    }
}

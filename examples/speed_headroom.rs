//! La synthèse suit-elle la lecture rapide ? Pour chaque voix installée :
//!
//! 1. débit brut du pool (secondes de parole produites par seconde) avec ses
//!    réglages par défaut : c'est la vitesse de lecture tenable sans fin ;
//! 2. lecture simulée à 3× (et aux vitesses demandées) avec la fenêtre de
//!    synthèse du lecteur : la phrase suivante est-elle prête à temps ?
//!
//! ```bash
//! cargo run --release --features audio-cpp --example speed_headroom
//! ```
//!
//! Rien n'est joué : la lecture est simulée par l'horloge. Les modèles sont
//! lus là où l'app les installe ; la configuration de l'utilisateur n'est pas
//! lue.

use std::sync::Arc;
use std::time::{Duration, Instant};

use sillon::config::Config;
use sillon::doc::DocumentBuilder;
use sillon::models;
use sillon::tts::audio_cpp::AudioCppEngine;
use sillon::tts::{Synthesizer, TtsEngine};

const TEXT: &str = "\
# La lecture rapide

Lire vite ne veut pas dire lire mal. Beaucoup de lecteurs écoutent leurs textes à une vitesse \
deux ou trois fois supérieure à la parole ordinaire, et comprennent tout.

Le cerveau s'habitue en quelques jours. Les premières minutes paraissent confuses, puis les mots \
se détachent, et la voix lente finit par sembler interminable.

## Ce qui compte

Une voix claire. Des pauses aux bons endroits. Une hauteur qui ne monte pas dans les aigus quand \
la vitesse augmente, sans quoi la fatigue arrive vite.

Les titres méritent un silence un peu plus long que les paragraphes. Les listes, un silence plus \
court. Ces repères aident à garder le fil, surtout quand on suit le texte des yeux.

## Un exemple

Le train de sept heures quarante part du quai numéro trois. Il dessert toutes les gares jusqu'à \
Lyon, puis continue vers Marseille sans arrêt. Les voyageurs pour Grenoble changent à Lyon \
Part-Dieu. Le wagon-restaurant se trouve au milieu du train.

En cas de retard de plus de trente minutes, une compensation est proposée. Il suffit de remplir \
le formulaire en ligne dans les soixante jours qui suivent le voyage.

## Pour finir

Écouter un texte plutôt que le lire n'est pas une facilité. C'est une autre manière d'y entrer, \
qui convient à certains mieux qu'à d'autres. L'important est de pouvoir choisir.
";

fn main() {
    let speeds: Vec<f32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let speeds = if speeds.is_empty() { vec![2.0, 3.0] } else { speeds };

    let installed: Vec<_> = models::available().into_iter().filter(|p| models::is_installed(p)).collect();
    if installed.is_empty() {
        println!("Aucune voix installée.");
        return;
    }
    for package in installed {
        let mut config = Config::default();
        config.engine.model_path = models::install_path(package).unwrap().to_string_lossy().into_owned();
        config.engine.family = package.family.to_string();
        config.voice.voice = models::default_voice(package.family).to_string();
        let mut builder = DocumentBuilder::new(config.code_policy());
        for block in sillon::input::split_blocks(TEXT) {
            builder.push_block(&block);
        }
        builder.finish();
        let sentences: Vec<(String, u64)> =
            builder.document().sentences.iter().map(|s| (s.text.clone(), s.trailing_pause_ms)).collect();
        let t = Instant::now();
        let engine: Arc<dyn TtsEngine> = match AudioCppEngine::load(&config.engine_settings()) {
            Ok(e) => Arc::new(e),
            Err(err) => {
                println!("{} : chargement impossible ({err})", package.id);
                continue;
            }
        };
        let sessions = config.sessions() as usize;
        let window = config.engine.prefetch as usize;
        println!(
            "\n== {} ({}) : chargé en {:.1} s, {} session(s), fenêtre {} phrases, {} phrases ==",
            package.id,
            package.family,
            t.elapsed().as_secs_f32(),
            sessions,
            window,
            sentences.len()
        );

        // Échauffement : la première phrase paie les graphes Metal.
        {
            let mut session = engine.open_session().expect("session");
            let _ = session.synthesize("Bonjour.", &config.tts_params());
        }

        // 1. Débit brut : tout le texte d'un coup, fenêtre ouverte.
        let synth = Synthesizer::new(Arc::clone(&engine), config.tts_params(), sessions, sentences.len());
        for (text, pause) in &sentences {
            synth.push(text.clone(), *pause);
        }
        let t = Instant::now();
        while synth.stats().ready + synth.stats().failed < sentences.len() {
            std::thread::sleep(Duration::from_millis(5));
        }
        let took = t.elapsed().as_secs_f64();
        let speech: f64 = (0..sentences.len()).filter_map(|i| synth.get(i)).map(|r| r.speech.as_secs_f64()).sum();
        let pauses: f64 = sentences.iter().map(|(_, p)| *p as f64 / 1000.0).sum();
        println!(
            "débit brut : {speech:.1} s de parole (+ {pauses:.1} s de pauses) en {took:.1} s → ×{:.2} temps réel \
             (parole seule), vitesse tenable sans fin ≈ {:.2}×",
            speech / took,
            speech / took
        );
        drop(synth);

        // 2. Lecture simulée : le curseur suit la phrase « qui sonne ».
        for &speed in &speeds {
            let synth = Synthesizer::new(Arc::clone(&engine), config.tts_params(), sessions, window);
            for (text, pause) in &sentences {
                synth.push(text.clone(), *pause);
            }
            let mut stalls = Vec::new();
            let start = Instant::now();
            for (id, (_, pause)) in sentences.iter().enumerate() {
                synth.set_cursor(id);
                let waited = Instant::now();
                let rendered = loop {
                    if let Some(r) = synth.get(id) {
                        break Some(r);
                    }
                    if synth.failure(id).is_some() {
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                };
                let wait = waited.elapsed();
                // La première phrase attend toujours : c'est le démarrage.
                if id > 0 && wait > Duration::from_millis(20) {
                    stalls.push((id, wait));
                }
                if let Some(r) = rendered {
                    let heard = r.speech.as_secs_f64() / speed as f64
                        + (*pause as f64 / 1000.0) / (speed as f64).sqrt();
                    std::thread::sleep(Duration::from_secs_f64(heard));
                }
            }
            let total_stall: Duration = stalls.iter().map(|(_, d)| *d).sum();
            println!(
                "lecture à {speed}× : {:.1} s, {} blanc(s), {:.1} s d'attente au total{}",
                start.elapsed().as_secs_f32(),
                stalls.len(),
                total_stall.as_secs_f32(),
                if stalls.is_empty() {
                    String::new()
                } else {
                    format!(
                        " — pire {:.2} s (phrase {})",
                        stalls.iter().map(|(_, d)| d.as_secs_f32()).fold(0.0, f32::max),
                        stalls.iter().max_by_key(|(_, d)| *d).unwrap().0
                    )
                }
            );
        }
    }
}

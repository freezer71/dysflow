//! Banc de mesure du démarrage : où passent les secondes avant le premier son.
//!
//! Reproduit le chemin critique réel — chargement différé du modèle, puis N
//! workers qui ouvrent leur session et synthétisent en concurrence — avec la
//! configuration de la machine. Diagnostic, pas régression : ignoré par défaut.
//!
//! ```sh
//! cargo test --release --features audio-cpp --test startup_timing -- --ignored --nocapture
//! ```

use sillon::config::Config;
use sillon::tts::{self, TtsEngine};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

const PHRASES: [&str; 3] = [
    "Le vingt-neuf août deux mille vingt-six, la réunion aura lieu à Paris.",
    "Deuxième phrase, de longueur comparable, pour occuper le second worker.",
    "Troisième phrase, de longueur comparable, pour occuper le dernier worker.",
];

#[test]
#[ignore = "banc de mesure : demande un modèle installé"]
fn where_the_startup_seconds_go() {
    let (mut config, _) = Config::load();
    // `MODEL=<chemin>` compare deux poids sans toucher à la configuration de
    // la machine ; `FAMILY=<famille>` l'accompagne quand le poids n'est pas
    // de la famille configurée. La voix suit alors la famille, comme dans le
    // lecteur : MagpieTTS refuse une voix de Supertonic.
    if let Ok(path) = std::env::var("MODEL") {
        config.engine.model_path = path;
    }
    if let Ok(family) = std::env::var("FAMILY") {
        config.engine.family = family;
        config.align_voice_with_family();
    }
    let mut settings = config.engine_settings();
    // `SESSIONS=<n>` mesure l'effet du nombre de sessions : c'est lui qui
    // décide combien de workers font la queue.
    if let Some(n) = std::env::var("SESSIONS").ok().and_then(|v| v.parse().ok()) {
        settings.sessions = n;
    }
    let params = config.tts_params();
    let sessions = settings.sessions.max(1) as usize;
    println!("\n=== configuration ===");
    println!("famille   : {}", settings.family);
    println!("backend   : {}", settings.backend);
    println!("sessions  : {sessions}");

    let quiet = sillon::logging::redirect_stderr();
    let t0 = Instant::now();
    let (engine, notice) = tts::build_engine(config.engine_preference(), &settings);
    let built = t0.elapsed();

    // `name()` porte « chargement… » tant que le GGUF n'est pas là : le poller
    // sépare le chargement du modèle de l'ouverture de la session.
    let probe = Instant::now();
    while engine.name().contains("chargement") {
        thread::sleep(std::time::Duration::from_millis(5));
    }
    let model_load = probe.elapsed();
    let probe = Instant::now();
    let mut first = engine.open_session().expect("session");
    let first_session = probe.elapsed();

    // Puis les workers restants se disputent le verrou d'ouverture, pendant que
    // le premier synthétise — exactement ce que fait le pool.
    let barrier = Arc::new(Barrier::new(sessions));
    let mut handles = Vec::new();
    for (i, phrase) in PHRASES.iter().enumerate().take(sessions).skip(1) {
        let engine: Arc<dyn TtsEngine> = Arc::clone(&engine);
        let barrier = Arc::clone(&barrier);
        let params = params.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            let start = Instant::now();
            let mut session = engine.open_session().expect("session");
            let opened = start.elapsed();
            let synth = Instant::now();
            let _ = session.synthesize(phrase, &params);
            (i, opened, synth.elapsed())
        }));
    }

    let contended = Instant::now();
    barrier.wait();
    let synth = Instant::now();
    let pcm = first.synthesize(PHRASES[0], &params).expect("synthèse");
    let first_synth = synth.elapsed();
    let to_first_sound = t0.elapsed();
    let audio = pcm.duration;

    let others: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    drop(quiet);

    println!("\n=== chemin critique ===");
    println!("build_engine (doit être ~0)      : {built:?}");
    println!("chargement du modèle             : {model_load:?}");
    println!("ouverture de la 1re session      : {first_session:?}");
    println!("synthèse de la 1re phrase        : {first_synth:?}");
    println!("→ premier son possible à         : {to_first_sound:?}");
    println!("   (pour {audio:?} d'audio)");

    println!("\n=== sessions concurrentes (départ à {:?}) ===", contended.elapsed());
    for (i, opened, synth) in others {
        println!("worker {i} : ouverture {opened:?}, synthèse {synth:?}");
    }
    if let Some(notice) = notice {
        println!("\nnote moteur : {notice}");
    }
}

/// Ce que coûte l'analyse du document — aujourd'hui **avant** que le
/// chargement du modèle ne démarre.
#[test]
#[ignore = "banc de mesure"]
fn what_the_parsing_costs_before_the_model_starts_loading() {
    use sillon::doc::DocumentBuilder;
    use sillon::input;

    let readme = std::fs::read_to_string("README.md").expect("README");
    for (label, text) in [
        ("README.md (13 Ko)", readme.clone()),
        ("×20 (260 Ko)", readme.repeat(20)),
        ("×100 (1,3 Mo)", readme.repeat(100)),
    ] {
        let start = Instant::now();
        let mut builder = DocumentBuilder::new(sillon::doc::builder::CodePolicy::Announce);
        for block in input::split_blocks(&text) {
            builder.push_block(&block);
        }
        let elapsed = start.elapsed();
        println!("{label:20} → {elapsed:?} ({} phrases)", builder.sentence_count());
    }
}

/// Ce que le pool coûte **après** que le modèle est prêt.
///
/// Un piège à éviter : la première session d'un processus paie la
/// spécialisation des pipelines Metal — environ une seconde — et toutes les
/// suivantes en profitent. Comparer deux réglages dans le même processus
/// mesure donc l'ordre des mesures, pas les réglages. D'où `WORKERS=` : une
/// mesure par processus, à comparer entre lancements.
///
/// ```sh
/// WORKERS=1 cargo test --release --features audio-cpp --test startup_timing \
///     -- --ignored --nocapture what_the_pool
/// ```
#[test]
#[ignore = "banc de mesure : demande un modèle installé"]
fn what_the_pool_costs_before_the_first_sound() {
    use sillon::tts::Synthesizer;
    use std::time::Duration;

    let (config, _) = Config::load();
    let settings = config.engine_settings();
    let params = config.tts_params();
    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(settings.sessions.max(1) as usize);

    let quiet = sillon::logging::redirect_stderr();
    let (engine, _) = tts::build_engine(config.engine_preference(), &settings);
    // Le chargement du modèle est commun à toutes les mesures : on l'attend
    // avant de lancer le chrono, pour ne compter que le pool.
    while engine.name().contains("chargement") {
        thread::sleep(Duration::from_millis(5));
    }
    let synth = Synthesizer::new(engine, params, workers, 8);
    for phrase in PHRASES {
        synth.push(phrase.to_string(), 0);
    }
    let start = Instant::now();
    while synth.get(0).is_none() {
        thread::sleep(Duration::from_millis(2));
    }
    let first = start.elapsed();
    while synth.get(PHRASES.len() - 1).is_none() {
        thread::sleep(Duration::from_millis(2));
    }
    let all = start.elapsed();
    drop(quiet);

    println!("\n=== du modèle prêt au premier son ===");
    println!("{workers} session(s) : 1re phrase {first:?}, les trois {all:?}");
}

/// Ce que pèse le modèle seul, et ce que coûte de rouvrir une session.
///
/// Un résident qui garderait le modèle entre deux lectures doit savoir ce
/// qu'il garde : si les sessions pèsent plus que les poids, c'est elles qu'il
/// faut fermer à l'inactivité — à condition qu'elles se rouvrent vite.
///
/// ```sh
/// STAGE=model /usr/bin/time -l target/release/deps/startup_timing-* \
///     --ignored --nocapture what_the_model_alone
/// ```
#[test]
#[ignore = "banc de mesure : demande un modèle installé"]
fn what_the_model_alone_weighs_and_what_reopening_costs() {
    use std::time::Duration;

    let (config, _) = Config::load();
    let settings = config.engine_settings();
    let params = config.tts_params();
    let quiet = sillon::logging::redirect_stderr();
    let (engine, _) = tts::build_engine(config.engine_preference(), &settings);
    while engine.name().contains("chargement") {
        thread::sleep(Duration::from_millis(5));
    }
    // `STAGE=model` s'arrête ici : l'empreinte mesurée par `time -l` est
    // alors celle des poids seuls.
    if std::env::var("STAGE").as_deref() == Ok("model") {
        drop(quiet);
        println!("\nmodèle chargé, aucune session ouverte");
        return;
    }

    let open = Instant::now();
    let mut first = engine.open_session().expect("session");
    let _ = first.synthesize(PHRASES[0], &params);
    let first_open = open.elapsed();
    drop(first);

    let open = Instant::now();
    let mut again = engine.open_session().expect("session");
    let _ = again.synthesize(PHRASES[1], &params);
    let reopen = open.elapsed();
    drop(again);
    drop(quiet);

    println!("\n=== rouvrir une session ===");
    println!("1re ouverture + synthèse : {first_open:?}");
    println!("réouverture + synthèse   : {reopen:?}");
}

//! Le parcours de la fenêtre, sans fenêtre : le contrôleur lit avec
//! Supertonic, on choisit Magpie dans « Modèles », et l'on regarde ce que la
//! lecture devient, tick après tick. Volume à zéro : rien ne sort.
//!
//! ```bash
//! cargo run --release --features audio-cpp --example switch_app
//! ```

use std::time::{Duration, Instant};

use sillon::app::config::Page;
use sillon::app::App;
use sillon::config::Config;
use sillon::doc::DocumentBuilder;
use sillon::{models, tts};

const DEMO: &str = include_str!("../docs/demo.md");

fn row_of(id: &str) -> usize {
    models::available().iter().position(|p| p.id == id).expect("modèle connu")
}

fn describe(app: &App) -> String {
    let stats = app.synth.stats();
    let phase = app.startup_phase().map(|p| p.label(&app.config.engine.family)).unwrap_or_default();
    let position = app
        .player
        .position()
        .map(|p| format!("phrase {} ({:.1} s){}", p.sentence_id + 1, p.speech.as_secs_f32(), if p.elapsed > Duration::ZERO { " en cours" } else { " au départ" }))
        .unwrap_or_else(|| "silence".to_string());
    format!(
        "{} · prêtes {} · en cours {} · refusées {} · file {} · {} · {}",
        app.engine_name(),
        stats.ready,
        stats.in_flight,
        stats.failed,
        app.player.queue_len(),
        position,
        phase
    )
}

/// Fait tourner le contrôleur `for_` secondes, et raconte ce qui change.
fn run(app: &mut App, for_: Duration, label: &str) {
    let start = Instant::now();
    let mut last = String::new();
    let mut silence_since: Option<Instant> = None;
    let mut first_sound: Option<Duration> = None;
    while start.elapsed() < for_ {
        app.tick();
        let now = describe(app);
        let speaking = app.player.position().is_some();
        if speaking && first_sound.is_none() {
            first_sound = Some(start.elapsed());
        }
        if !speaking && app.player.is_playing() && first_sound.is_some() {
            silence_since.get_or_insert(Instant::now());
        } else if let Some(since) = silence_since.take() {
            println!("  [{label} +{:.1} s] trou de {:.1} s", start.elapsed().as_secs_f32(), since.elapsed().as_secs_f32());
        }
        if now != last {
            println!("[{label} +{:.1} s] {now}", start.elapsed().as_secs_f32());
            last = now;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    match first_sound {
        Some(at) => println!("=> {label} : premier son après {:.1} s", at.as_secs_f32()),
        None => println!("=> {label} : aucun son en {:.0} s", for_.as_secs_f32()),
    }
}

fn main() {
    let (mut config, _) = Config::load();
    config.voice.volume = 0.0;
    config.reading.autoplay = true;
    let mut builder = DocumentBuilder::new(config.code_policy());
    for block in sillon::input::split_blocks(DEMO) {
        builder.push_block(&block);
    }
    let (engine, notice) = tts::build_engine(config.engine_preference(), &config.engine_settings());
    let mut app = App::new(builder, None, engine, config, "demo.md".to_string(), notice.into_iter().collect(), sillon::audio::Player::new());
    app.player.set_volume(0.0);
    println!("départ : {}", app.engine_name());
    run(&mut app, Duration::from_secs(4), "supertonic");

    // « Utiliser » Magpie, comme dans Réglages › Modèles.
    app.open_config();
    app.screen.section = 0;
    app.screen.page = Page::Detail;
    app.screen.row = row_of("magpie_tts_q8_0");
    let t = Instant::now();
    app.config_activate();
    println!("config_activate (bloquant) : {:.3} s — {}", t.elapsed().as_secs_f32(), app.screen.notice);
    app.toggle_config();
    println!("après retour à la lecture : playing = {}", app.player.is_playing());
    if !app.player.is_playing() {
        app.toggle_play();
    }
    run(&mut app, Duration::from_secs(50), "→ magpie");

    // Et retour.
    app.open_config();
    app.screen.section = 0;
    app.screen.page = Page::Detail;
    app.screen.row = row_of("supertonic_3_orig");
    let t = Instant::now();
    app.config_activate();
    println!("config_activate (bloquant) : {:.3} s — {}", t.elapsed().as_secs_f32(), app.screen.notice);
    app.toggle_config();
    println!("après retour à la lecture : playing = {}", app.player.is_playing());
    if !app.player.is_playing() {
        app.toggle_play();
    }
    run(&mut app, Duration::from_secs(15), "→ supertonic");
    std::process::exit(0);
}

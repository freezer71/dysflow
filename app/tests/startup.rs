//! Le premier lancement, en headless : l'installation de Kokoro demandée au
//! cœur (un téléchargement factice, jamais le réseau), la voix de repli, la
//! langue du système et l'absence de GPU. Aucune fenêtre réelle, aucun son ;
//! les répertoires du lecteur sont ceux des essais (feature `test-paths`),
//! la configuration s'écrit dans un dossier temporaire.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::audio::Player;
use dysflow::config::Config;
use dysflow::i18n::{with_ui_lang, UiLang};
use dysflow::models::Download;
use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::settings::DownloadView;
use dysflow::reader::view::{Snapshot, VoiceState};
use dysflow::reader::Reader;
use dysflow::tts::silent::SilentEngine;
use dysflow::tts::TtsEngine;
use dysflow_app::session::Session;
use dysflow_app::settings::ui_lang_choice;
use dysflow_app::startup::{download_label, first_launch, no_gpu_message, no_window, panic_reason};
use dysflow_app::transport::can_play;
use dysflow_app::window::MainWindow;
use gpui::{px, size, Entity, TestAppContext, VisualTestContext};

/// Un dossier temporaire propre au test, retiré à la fin.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("dysflow-app-startup-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Attend, sans dormir, que le fil du lecteur ait publié l'état voulu.
fn wait_for(session: &Session, what: &str, ok: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(&session.snapshot()) {
        assert!(Instant::now() < deadline, "{what} : jamais publié");
        std::thread::yield_now();
    }
}

/// Un lecteur muet qui écrit sur disque, comme celui de l'app (sinon il ne
/// télécharge rien), mais dans un dossier temporaire ; le téléchargement de
/// Kokoro est `start` : jamais le réseau.
fn installing_reader(
    config: PathBuf,
    start: fn(&'static dysflow::models::ModelPackage) -> anyhow::Result<Download>,
) -> Reader {
    let mut reader = silent_reader();
    reader.set_persistent(true);
    reader.set_config_path(Some(config));
    // Comme `app_reader` quand l'app fournit un calcul (`--features mlx`) :
    // un Kokoro est alors chargeable, donc à installer. La voix reste muette.
    reader.set_mlx_compute(true);
    reader.set_download_start(start);
    reader
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// La fenêtre principale dans une fenêtre de test, comme dans `run`.
fn window<'a>(
    cx: &'a mut TestAppContext,
    session: &Arc<Session>,
) -> (Entity<MainWindow>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::default();
    let shared = Arc::clone(session);
    let out = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let main = MainWindow::build(shared, None, window, cx);
        *out.borrow_mut() = Some(main.clone());
        gpui_component::Root::new(main, window, cx)
    });
    cx.simulate_resize(size(px(800.0), px(500.0)));
    cx.run_until_parked();
    draw(cx);
    let main = slot.borrow().clone().expect("la fenêtre principale");
    (main, cx)
}

#[test]
fn download_label_shows_percent_and_failure() {
    let running = DownloadView {
        id: "kokoro".into(),
        received: 50,
        total: 200,
        failed: None,
    };
    let fr = with_ui_lang(UiLang::Fr, || download_label(&running));
    assert!(fr.contains("25 %"), "{fr}");
    let en = with_ui_lang(UiLang::En, || download_label(&running));
    assert!(en.contains("25%"), "{en}");
    // Rien de reçu, taille inconnue : 0 %, sans division par zéro.
    let empty = DownloadView {
        total: 0,
        received: 0,
        ..running.clone()
    };
    assert!(with_ui_lang(UiLang::Fr, || download_label(&empty)).contains("0 %"));
    // Fini à 100 % au plus, même si le serveur en envoie plus.
    let over = DownloadView {
        received: 300,
        ..running.clone()
    };
    assert!(with_ui_lang(UiLang::Fr, || download_label(&over)).contains("100 %"));

    let failed = DownloadView {
        failed: Some("réseau coupé".into()),
        ..running
    };
    let fr = with_ui_lang(UiLang::Fr, || download_label(&failed));
    assert!(fr.contains("réseau coupé"), "{fr}");
    assert!(fr.contains("Réessayer"), "{fr}");
    assert!(
        !fr.contains('%'),
        "un échec ne montre pas d'avancement : {fr}"
    );
    let en = with_ui_lang(UiLang::En, || download_label(&failed));
    assert!(en.contains("réseau coupé") && en.contains("Retry"), "{en}");
}

static INSTALLS: AtomicUsize = AtomicUsize::new(0);

#[gpui::test]
fn first_launch_asks_core_to_install(cx: &mut TestAppContext) {
    let scratch = Scratch::new("first-launch");
    let config = scratch.0.join("config.toml");
    let session = Arc::new(Session::new(ReaderHandle::spawn(move || {
        installing_reader(config, |package| {
            INSTALLS.fetch_add(1, Ordering::SeqCst);
            Ok(Download::fake(package, false))
        })
    })));
    assert!(session.snapshot().download.is_none());
    let (main, cx) = window(cx, &session);

    // La requête part sans attendre ; sa réponse s'attend hors du fil de
    // l'interface.
    let asked = Instant::now();
    let reply = first_launch(&session);
    assert!(
        asked.elapsed() < Duration::from_millis(100),
        "first_launch n'attend pas"
    );
    main.update(cx, |m, cx| m.watch_voice_setup(reply, cx));
    cx.run_until_parked();
    wait_for(&session, "le téléchargement", |s| s.download.is_some());

    let d = session.snapshot().download.expect("un téléchargement");
    assert!(
        d.id.starts_with("kokoro"),
        "le Kokoro recommandé : {}",
        d.id
    );
    assert_eq!(d.failed, None);
    assert_eq!(INSTALLS.load(Ordering::SeqCst), 1);

    // La barre d'état dit l'avancement, et que la voix de repli lit.
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    let line = main.read_with(cx, |m, _| m.status_line());
    assert!(line.contains(&download_label(&d)), "{line}");
    assert!(line.contains("0%") || line.contains("0 %"), "{line}");

    // Un deuxième lancement ne double pas un téléchargement qui court.
    first_launch(&session)
        .wait()
        .expect("une réponse")
        .expect("sans erreur");
    assert_eq!(INSTALLS.load(Ordering::SeqCst), 1);
}

static REFUSED: AtomicUsize = AtomicUsize::new(0);

#[gpui::test]
fn a_refused_install_is_shown_and_can_be_retried(cx: &mut TestAppContext) {
    let scratch = Scratch::new("refused");
    let config = scratch.0.join("config.toml");
    let session = Arc::new(Session::new(ReaderHandle::spawn(move || {
        installing_reader(config, |_| {
            REFUSED.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("serveur injoignable"))
        })
    })));
    let (main, cx) = window(cx, &session);
    let reply = first_launch(&session);
    main.update(cx, |m, cx| m.watch_voice_setup(reply, cx));
    cx.run_until_parked();
    let line = main.read_with(cx, |m, _| m.status_line());
    assert!(line.contains("serveur injoignable"), "{line}");
    assert!(
        line.contains("Retry") || line.contains("Réessayer"),
        "{line}"
    );
    assert!(main.read_with(cx, |m, _| m.can_retry_voice()));
    assert_eq!(REFUSED.load(Ordering::SeqCst), 1);

    // « Réessayer » redemande l'installation au cœur.
    main.update(cx, |m, cx| m.retry_voice(cx));
    cx.run_until_parked();
    assert_eq!(REFUSED.load(Ordering::SeqCst), 2);
    draw(cx);
}

/// Un lecteur dont la voix voulue a échoué : il parle par la voix de repli
/// (muette ici) et le dit.
fn fallback_reader() -> Reader {
    let mut config = Config::default();
    config.reading.autoplay = false;
    config.app.keep_voice_minutes = -1;
    let mut reader = Reader::new(
        config,
        Box::new(|_| {
            dysflow::tts::fallback::explained(
                Arc::new(SilentEngine) as Arc<dyn TtsEngine>,
                "Kokoro absent".into(),
            )
        }),
        Box::new(|_: &str| Player::silent()),
        Instant::now(),
    );
    reader.set_persistent(false);
    reader
}

#[gpui::test]
fn reading_starts_on_fallback_voice(cx: &mut TestAppContext) {
    let session = Arc::new(Session::new(ReaderHandle::spawn(fallback_reader)));
    session.send(Request::OpenText {
        markdown: "Une phrase. Une autre phrase.".into(),
        source: "test".into(),
    });
    wait_for(&session, "la voix de repli", |s| {
        s.has_document && matches!(s.voice, VoiceState::Fallback { .. })
    });
    let snapshot = session.snapshot();
    assert!(can_play(&snapshot), "la voix de repli permet de lire");

    let (main, cx) = window(cx, &session);
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    draw(cx);
    let line = main.read_with(cx, |m, _| m.status_line());
    assert!(line.contains("Kokoro absent"), "{line}");
    assert!(
        line.contains("fallback") || line.contains("repli"),
        "la barre dit le repli : {line}"
    );

    cx.simulate_keystrokes("space");
    wait_for(&session, "la lecture", |s| s.playing);
    session.send(Request::Command(Command::TogglePlay));
    wait_for(&session, "la pause", |s| !s.playing);
}

#[test]
fn no_gpu_message_is_bilingual() {
    let message = no_gpu_message("accessing a compatible Metal device");
    assert!(message.contains("accessing a compatible Metal device"));
    // Les deux langues, quelle que soit celle de l'interface : la fenêtre ne
    // s'est pas ouverte, rien d'autre ne le dira.
    for lang in [UiLang::En, UiLang::Fr] {
        let m = with_ui_lang(lang, || no_gpu_message("x"));
        assert!(m.contains("window"), "anglais : {m}");
        assert!(m.contains("fenêtre"), "français : {m}");
    }
}

#[test]
fn no_window_writes_the_message_and_exits_with_1() {
    let mut err = Vec::new();
    let code = no_window("no display server", &mut err);
    assert_eq!(code, 1);
    let written = String::from_utf8(err).expect("de l'UTF-8");
    assert_eq!(written.trim_end(), no_gpu_message("no display server"));
    assert!(written.ends_with('\n'));
}

#[test]
fn system_language_prefers_environment_then_platform() {
    let unset = |_: &str| None::<String>;
    let fr_env = |k: &str| (k == "LANG").then(|| "fr_FR.UTF-8".to_string());
    let en_env = |k: &str| (k == "LC_ALL").then(|| "en_US.UTF-8".to_string());
    let empty_env = |k: &str| (k == "LANG").then(String::new);
    let fr_platform = || Some("fr-FR".to_string());
    let en_platform = || Some("en-US".to_string());
    let no_platform = || None::<String>;

    // L'environnement défini l'emporte sur la plateforme.
    assert_eq!(ui_lang_choice("", fr_env, en_platform), UiLang::Fr);
    assert_eq!(ui_lang_choice("", en_env, fr_platform), UiLang::En);
    // Absent (Finder, Windows) : la langue de la plateforme.
    assert_eq!(ui_lang_choice("", unset, fr_platform), UiLang::Fr);
    assert_eq!(ui_lang_choice("", unset, en_platform), UiLang::En);
    // Une variable vide compte comme absente.
    assert_eq!(ui_lang_choice("", empty_env, fr_platform), UiLang::Fr);
    // « C », « POSIX » (et « C.UTF-8 ») ne disent aucune langue : comme
    // absentes, la plateforme décide.
    for tag in ["C", "POSIX", "C.UTF-8"] {
        let c_env = move |k: &str| (k == "LANG").then(|| tag.to_string());
        assert_eq!(ui_lang_choice("", c_env, fr_platform), UiLang::Fr, "{tag}");
    }
    let c_then_fr = |k: &str| match k {
        "LC_ALL" => Some("C".to_string()),
        "LANG" => Some("fr_FR.UTF-8".to_string()),
        _ => None,
    };
    assert_eq!(ui_lang_choice("", c_then_fr, en_platform), UiLang::Fr);
    // Ni l'un ni l'autre : l'anglais.
    assert_eq!(ui_lang_choice("", unset, no_platform), UiLang::En);
    // Une langue choisie ne regarde ni l'un ni l'autre.
    assert_eq!(ui_lang_choice("en", fr_env, fr_platform), UiLang::En);
    assert_eq!(ui_lang_choice("fr", en_env, en_platform), UiLang::Fr);
    let never = || -> Option<String> { panic!("la plateforme n'est pas lue") };
    assert_eq!(ui_lang_choice("fr", unset, never), UiLang::Fr);
    assert_eq!(ui_lang_choice("", fr_env, never), UiLang::Fr);
}

#[test]
fn the_tests_never_touch_the_user_s_folders() {
    // Garde-fou : la feature `test-paths` est bien active dans ces essais
    // (modèles, positions, cache dans le dossier temporaire du processus).
    assert!(dysflow::paths::scratch_paths());
}

#[test]
fn panic_reason_reads_the_panic_text() {
    let text: Box<dyn std::any::Any + Send> = Box::new("failed to initialize DirectX");
    assert_eq!(panic_reason(text.as_ref()), "failed to initialize DirectX");
    let formatted: Box<dyn std::any::Any + Send> = Box::new(String::from("X11: refused"));
    assert_eq!(panic_reason(formatted.as_ref()), "X11: refused");
    let other: Box<dyn std::any::Any + Send> = Box::new(42u8);
    assert!(!panic_reason(other.as_ref()).is_empty());
}

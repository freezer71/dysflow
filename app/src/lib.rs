//! Lecteur Dysflow en interface graphique (GPUI).

pub mod contents;
pub mod editor;
pub mod fonts;
pub mod help;
pub mod inputs;
pub mod keymap;
pub mod layout;
pub mod reading;
pub mod search;
pub mod session;
pub mod settings;
pub mod startup;
pub mod theme;
pub mod transport;
pub mod window;

use std::borrow::Cow;
use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dysflow::config::{Config, Theme};
use dysflow::reader::build::app_reader;
use dysflow::reader::handle::ReaderHandle;
use gpui::{
    px, size, App, AppContext, AssetSource, Bounds, Entity, SharedString, WindowBounds,
    WindowOptions,
};

use crate::session::Session;
use crate::window::MainWindow;

// Les icônes de la barre qui ne sont pas dans le jeu par défaut de
// gpui-component.
gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        SkipBack,
        SkipForward,
        ClipboardPaste,
        CircleQuestionMark,
        TableOfContents,
        Pencil,
        Monitor,
        Download
    ]
);

/// Les icônes de l'app : celles de gpui-component, plus celles de la barre.
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match gpui_kit_assets::Assets.load(path)? {
            Some(found) => Ok(Some(found)),
            None => ExtraIcons.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut all = gpui_kit_assets::Assets.list(path)?;
        all.extend(ExtraIcons.list(path)?);
        Ok(all)
    }
}

/// Ce que l'app pose avant sa première fenêtre : gpui-component, les
/// polices embarquées (`fonts::register`), puis le thème choisi.
pub fn init(cx: &mut App, theme: Theme) {
    gpui_component::init(cx);
    fonts::register(cx);
    theme::install(theme, cx);
}

/// À la sortie de l'app, la position de lecture s'écrit explicitement
/// (`Session::remember_position`, donc `Reader::close`) : le fil du lecteur
/// ne s'arrête pas de lui-même avant que le processus ne sorte. La requête
/// part aussitôt ; sa réponse s'attend sur l'exécuteur d'arrière-plan, et
/// GPUI n'attend ce futur que `gpui::SHUTDOWN_TIMEOUT` (200 ms) au plus :
/// le fil de l'interface n'est jamais bloqué plus longtemps.
pub fn remember_position_on_quit(session: Arc<Session>, cx: &mut App) {
    cx.on_app_quit(move |cx| {
        let reply = session.remember_position();
        let written = cx.background_executor().spawn(async move {
            let _ = reply.wait();
        });
        written
    })
    .detach();
}

/// Aucune fenêtre ne s'ouvre : `startup::no_window` (le message sur la
/// sortie d'erreur), la boîte du système sous Windows
/// (`startup::system_alert`), puis la sortie avec le code 1.
fn exit_without_window(err: &str) -> ! {
    let code = startup::no_window(err, &mut std::io::stderr());
    startup::system_alert(&startup::no_gpu_message(err));
    std::process::exit(code)
}

/// Ouvre l'application et sa fenêtre principale ; `path` (l'argument de la
/// ligne de commande) s'ouvre aussitôt, sans bloquer l'interface (voir
/// `MainWindow::new`). La fenêtre ouverte, l'installation de la voix est
/// demandée au cœur (`startup::first_launch`) : la voix de repli lit en
/// attendant. La dernière fenêtre fermée quitte l'app. Sans fenêtre
/// possible (pas de GPU, pas d'écran) : `startup::no_gpu_message` sur la
/// sortie d'erreur (et, sous Windows, sans console, dans une boîte du
/// système), et le code de sortie 1.
pub fn run(path: Option<PathBuf>) {
    // Avant tout, avant le moindre fil : ONNX Runtime lit cette variable à
    // son premier environnement (voir `tts::kokoro::onnx::NO_TELEMETRY`).
    // Édition 2021 : `set_var` n'est pas `unsafe`, mais n'est sûr que sans
    // autre fil qui lirait l'environnement. Ici aucun n'existe encore : ni
    // le fil du lecteur (`ReaderHandle::spawn`, plus bas), ni ceux de GPUI
    // (`application()`), ni ceux d'un téléchargement.
    std::env::set_var("ORT_DISABLE_TELEMETRY", "1");
    // La langue et le thème se lisent avant la fenêtre : elle s'ouvre
    // directement dans les bons, sans changer sous les yeux. Un petit
    // fichier lu une fois, avant que l'interface n'existe ; le lecteur
    // reçoit la même lecture.
    let loaded = Config::load();
    dysflow::i18n::set_ui_lang(settings::ui_lang_for(&loaded.0.app.ui_language));
    let theme = loaded.0.app.theme;
    // `AppCompute::default()` : Kokoro MLX en Rust avec la feature `mlx`
    // (Mac), Kokoro ONNX avec `onnx`, sinon la voix du système seule.
    let session = Arc::new(Session::new(ReaderHandle::spawn(move || {
        app_reader(loaded, dysflow::tts::AppCompute::default())
    })));
    // GPUI panique quand il ne peut pas démarrer (Windows sans DirectX,
    // Linux dont le serveur d'affichage est injoignable), avant le rappel
    // ci-dessous : le crochet le dit dans les deux langues, puis la panique
    // suit son cours (release : `panic = "abort"`, le processus s'arrête
    // sur un abort, pas avec le code 1).
    startup::install_startup_hook(|message| {
        let _ = writeln!(std::io::stderr(), "{message}");
        startup::system_alert(message);
    });
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            // GPUI a démarré : les paniques suivantes ne parlent plus de GPU.
            startup::remove_startup_hook();
            // Linux sans serveur d'affichage : GPUI démarre « sans tête » et
            // ouvrirait une fenêtre invisible, l'app tournerait sans que
            // personne ne la voie.
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if cx.compositor_name() == "headless" {
                exit_without_window(dysflow::tr!(
                    "no display server: WAYLAND_DISPLAY and DISPLAY are unset",
                    "aucun serveur d'affichage : WAYLAND_DISPLAY et DISPLAY sont absents"
                ));
            }
            init(cx, theme);
            remember_position_on_quit(Arc::clone(&session), cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(900.0), px(680.0)),
                    cx,
                ))),
                ..Default::default()
            };
            let slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::default();
            let out = Rc::clone(&slot);
            let shared = Arc::clone(&session);
            let opened = cx.open_window(options, move |window, cx| {
                let main = MainWindow::build(shared, path, window, cx);
                *out.borrow_mut() = Some(main.clone());
                cx.new(|cx| gpui_component::Root::new(main, window, cx))
            });
            // Ici, l'échec de la fenêtre est une erreur rendue : Metal
            // (« accessing a compatible Metal device »), wgpu sous Linux,
            // Windows démarré sans tête (par SSH : « cannot open windows
            // while headless »). L'absence de DirectX, elle, est une panique
            // au démarrage (crochet plus haut). `cx.quit()` ne rendrait pas
            // de code 1 (sous macOS, `terminate:` sort avec 0) : on sort ici.
            if let Err(err) = opened {
                exit_without_window(&format!("{err:#}"));
            }
            // Seulement une fois la fenêtre là : sans elle, télécharger un
            // modèle de voix ne servirait à rien.
            let reply = startup::first_launch(&session);
            if let Some(main) = slot.borrow().clone() {
                main.update(cx, |m, cx| m.watch_voice_setup(reply, cx));
            }
            cx.activate(true);
        });
}

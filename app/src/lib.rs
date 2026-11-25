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

/// Ouvre l'application et sa fenêtre principale ; `path` (l'argument de la
/// ligne de commande) s'ouvre aussitôt, sans bloquer l'interface (voir
/// `MainWindow::new`). La fenêtre ouverte, l'installation de la voix est
/// demandée au cœur (`startup::first_launch`) : la voix de repli lit en
/// attendant. La dernière fenêtre fermée quitte l'app. Sans fenêtre
/// possible (pas de GPU, pas d'écran) : `startup::no_gpu_message` sur la
/// sortie d'erreur, et le code de sortie 1.
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
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            // Linux sans serveur d'affichage : GPUI démarre « sans tête » et
            // ouvrirait une fenêtre invisible, l'app tournerait sans que
            // personne ne la voie.
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if cx.compositor_name() == "headless" {
                std::process::exit(startup::no_window(
                    "no display server: WAYLAND_DISPLAY and DISPLAY are unset",
                    &mut std::io::stderr(),
                ));
            }
            init(cx, theme);
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
            // L'échec de GPUI est une erreur rendue (Metal : « accessing a
            // compatible Metal device », DirectX, wgpu…), pas une panique.
            // `cx.quit()` ne rendrait pas de code 1 (sous macOS,
            // `terminate:` sort avec 0) : on sort ici.
            if let Err(err) = opened {
                std::process::exit(startup::no_window(
                    &format!("{err:#}"),
                    &mut std::io::stderr(),
                ));
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

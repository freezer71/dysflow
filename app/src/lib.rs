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
pub mod theme;
pub mod transport;
pub mod window;

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use dysflow::config::{Config, Theme};
use dysflow::reader::build::app_reader;
use dysflow::reader::handle::ReaderHandle;
use gpui::{
    px, size, App, AppContext, AssetSource, Bounds, SharedString, WindowBounds, WindowOptions,
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
/// `MainWindow::new`). La dernière fenêtre fermée quitte l'app.
pub fn run(path: Option<PathBuf>) {
    // Avant le moindre fil : ONNX Runtime lit cette variable à son premier
    // environnement (voir `tts::kokoro::onnx::NO_TELEMETRY`).
    std::env::set_var("ORT_DISABLE_TELEMETRY", "1");
    // La langue et le thème se lisent avant la fenêtre : elle s'ouvre
    // directement dans les bons, sans changer sous les yeux. Un petit
    // fichier lu une fois, avant que l'interface n'existe ; le lecteur
    // reçoit la même lecture.
    let loaded = Config::load();
    dysflow::i18n::set_ui_lang(settings::ui_lang_for(&loaded.0.app.ui_language));
    let theme = loaded.0.app.theme;
    let session = Arc::new(Session::new(ReaderHandle::spawn(move || {
        app_reader(loaded, dysflow::tts::AppCompute::default())
    })));
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
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
            cx.open_window(options, move |window, cx| {
                let main = MainWindow::build(session, path, window, cx);
                cx.new(|cx| gpui_component::Root::new(main, window, cx))
            })
            .expect("ouverture de la fenêtre");
            cx.activate(true);
        });
}

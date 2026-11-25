//! Lecteur Dysflow en interface graphique (GPUI).

pub mod keymap;
pub mod layout;
pub mod reading;
pub mod session;

use std::path::PathBuf;

use gpui::{div, App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window};

/// La vue racine : tant que rien n'est ouvert, elle le dit.
pub struct RootView {
    message: &'static str,
}

impl RootView {
    pub fn new() -> Self {
        RootView {
            message: dysflow::tr!("Nothing to read", "Rien à lire"),
        }
    }

    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl Default for RootView {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.message)
    }
}

/// Ouvre l'application. Le document à lire sera pris en charge par les
/// tâches suivantes ; pour l'instant la fenêtre est vide.
pub fn run(_path: Option<PathBuf>) {
    gpui_platform::application().run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let view = cx.new(|_| RootView::new());
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("ouverture de la fenêtre");
        cx.activate(true);
    });
}

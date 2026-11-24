//! Essai de fumée headless : la vue racine se crée et se rend dans la
//! plateforme de test de GPUI, sans fenêtre réelle.

use dysflow_app::RootView;
use gpui::TestAppContext;

#[gpui::test]
fn opens_an_empty_root_view(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_window, _cx| RootView::new());
    // Un rendu complet de la vue (mise en page et dessin) sans écran.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let message = view.read_with(cx, |view, _| view.message());
    assert!(!message.is_empty());
}

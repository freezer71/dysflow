//! Essai de fumée headless : la fenêtre principale se crée et se rend dans
//! la plateforme de test de GPUI, sans fenêtre réelle, sur un lecteur muet.

use std::sync::Arc;

use dysflow::reader::build::silent_reader;
use dysflow::reader::handle::ReaderHandle;
use dysflow_app::session::Session;
use dysflow_app::window::MainWindow;
use gpui::TestAppContext;

#[gpui::test]
fn opens_an_empty_main_window(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let session = Arc::new(Session::new(ReaderHandle::spawn(silent_reader)));
    let (view, cx) = cx.add_window_view(|window, cx| MainWindow::new(session, None, window, cx));
    // Un rendu complet de la fenêtre (mise en page et dessin) sans écran.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let renders = view.read_with(cx, |view, _| view.renders());
    assert!(renders >= 1);
    view.read_with(cx, |view, _| assert_eq!(view.error(), None));
}

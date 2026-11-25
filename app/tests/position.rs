//! La position de lecture à la fermeture de la fenêtre (⌘W) et à la sortie
//! de l'app, en headless : un vrai lecteur muet sur son fil, la plateforme
//! de test de GPUI. Les positions s'écrivent dans le dossier des essais
//! (feature `test-paths`), jamais chez l'utilisateur ; ce fichier est son
//! propre processus, seul à écrire `positions.json` dans son dossier.
//!
//! Le premier lecteur reste vivant pendant la vérification : sa fin
//! (`ReaderHandle::drop`, qui écrit aussi la position) ne peut donc pas
//! être ce qui l'a écrite.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::position::{document_identity, PositionStore};
use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::view::Snapshot;
use dysflow_app::inputs::open_path;
use dysflow_app::session::Session;
use dysflow_app::window::MainWindow;
use gpui::{px, size, Entity, TestAppContext, VisualTestContext};

/// Un dossier temporaire propre au test, retiré à la fin.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dysflow-app-position-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        Scratch(dir)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).expect("fichier temporaire");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Les deux essais écrivent le même `positions.json` (celui du processus) :
/// chaque lecteur relit le fichier à l'ouverture et l'écrit entier, deux
/// essais en même temps pourraient s'effacer l'un l'autre. Ils passent l'un
/// après l'autre.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

const TEXT: &str = "Un deux trois. Quatre cinq six. Sept huit neuf. Dix onze douze.";

fn session() -> Arc<Session> {
    Arc::new(Session::new(ReaderHandle::spawn(silent_reader)))
}

fn wait_for(session: &Session, what: &str, ok: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(&session.snapshot()) {
        assert!(Instant::now() < deadline, "{what} : jamais publié");
        std::thread::yield_now();
    }
}

/// Ouvre `path` et va à la phrase `sentence`.
fn open_at(session: &Session, path: &Path, sentence: usize) {
    open_path(session, path).expect("le fichier s'ouvre");
    session.send(Request::Command(Command::SeekTo(sentence)));
    wait_for(session, "la phrase", |s| s.sentence == sentence);
}

/// La position retenue pour `path` dans `positions.json` (dossier des
/// essais), relue du disque ; attend qu'elle y soit, 5 s au plus.
fn written_position(path: &Path) -> Option<usize> {
    let identity = document_identity(path);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(p) = PositionStore::load().get(&identity) {
            return Some(p.sentence_id);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Un second lecteur rouvre `path` : il reprend à la position relue.
fn reopened_sentence(path: &Path) -> usize {
    let other = session();
    open_path(&other, path).expect("le fichier se rouvre");
    other
        .ask(|r| dysflow::reader::view::snapshot(r).sentence)
        .expect("le lecteur répond")
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

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

fn command(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

#[gpui::test]
fn closing_the_window_writes_the_reading_position(cx: &mut TestAppContext) {
    let _serial = serial();
    let dir = Scratch::new("fenetre");
    let path = dir.file("fenetre.md", TEXT);
    let session = session();
    open_at(&session, &path, 2);
    let (_main, cx) = window(cx, &session);
    assert_eq!(written_position_now(&path), None, "rien d'écrit avant");
    cx.simulate_keystrokes(&command("w"));
    cx.run_until_parked();
    assert!(cx.windows().is_empty(), "⌘W ferme la fenêtre");
    assert_eq!(written_position(&path), Some(2), "la position est écrite");
    assert_eq!(reopened_sentence(&path), 2, "la position est relue");
    // Le premier lecteur vivait encore : c'est la fermeture qui a écrit.
    drop(session);
}

#[gpui::test]
fn quitting_the_app_writes_the_reading_position(cx: &mut TestAppContext) {
    let _serial = serial();
    let dir = Scratch::new("sortie");
    let path = dir.file("sortie.md", TEXT);
    let session = session();
    open_at(&session, &path, 1);
    cx.update(|cx| dysflow_app::remember_position_on_quit(Arc::clone(&session), cx));
    assert_eq!(written_position_now(&path), None, "rien d'écrit avant");
    let quitting = Instant::now();
    cx.quit();
    assert!(
        quitting.elapsed() < Duration::from_secs(2),
        "la sortie n'attend que brièvement"
    );
    assert_eq!(written_position(&path), Some(1), "la position est écrite");
    assert_eq!(reopened_sentence(&path), 1, "la position est relue");
    drop(session);
}

/// La position retenue pour `path`, telle quelle, sans attendre.
fn written_position_now(path: &Path) -> Option<usize> {
    PositionStore::load()
        .get(&document_identity(path))
        .map(|p| p.sentence_id)
}

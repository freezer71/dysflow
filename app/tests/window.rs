//! La fenêtre principale, en headless : un vrai lecteur muet
//! (`silent_reader`) sur son fil, la plateforme de test de GPUI. Aucune
//! fenêtre réelle, aucune boîte d'ouverture réelle, aucun vrai
//! presse-papiers, aucun son. Les fichiers viennent d'un dossier temporaire.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::view::Snapshot;
use dysflow_app::help;
use dysflow_app::inputs::{self, key_press, open_path, paste_with};
use dysflow_app::keymap::{action, Action, KeyPress, Modifier};
use dysflow_app::session::Session;
use dysflow_app::window::{status_text, MainWindow};
use gpui::{
    point, px, size, Entity, ExternalPaths, FileDropEvent, Keystroke, Modifiers, TestAppContext,
    VisualTestContext,
};

/// Un dossier temporaire propre au test, retiré à la fin.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("dysflow-app-window-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        Scratch(dir)
    }

    fn file(&self, name: &str, content: &[u8]) -> PathBuf {
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

fn session() -> Arc<Session> {
    Arc::new(Session::new(ReaderHandle::spawn(silent_reader)))
}

/// Attend, sans dormir, que le fil du lecteur ait publié l'état voulu ;
/// cinq secondes au plus.
fn wait_for(session: &Session, what: &str, ok: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ok(&session.snapshot()) {
        assert!(Instant::now() < deadline, "{what} : jamais publié");
        std::thread::yield_now();
    }
}

fn open_text(session: &Session, markdown: &str) {
    session.send(Request::OpenText {
        markdown: markdown.into(),
        source: "test".into(),
    });
    wait_for(session, "le texte", |s| s.has_document && s.revision >= 1);
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// Un tour de la boucle de rafraîchissement (horloge simulée), puis un rendu.
fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    draw(cx);
}

/// La fenêtre principale dans une fenêtre de test de 800 × 500, sous la
/// racine de gpui-component, comme dans `run`.
fn window<'a>(
    cx: &'a mut TestAppContext,
    session: &Arc<Session>,
    initial: Option<PathBuf>,
) -> (Entity<MainWindow>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::default();
    let shared = Arc::clone(session);
    let out = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let main = MainWindow::build(shared, initial, window, cx);
        *out.borrow_mut() = Some(main.clone());
        gpui_component::Root::new(main, window, cx)
    });
    cx.simulate_resize(size(px(800.0), px(500.0)));
    cx.run_until_parked();
    draw(cx);
    let main = slot.borrow().clone().expect("la fenêtre principale");
    (main, cx)
}

fn status(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> String {
    main.read_with(cx, |m, _| m.status_line())
}

/// La touche de commande du système, pour `simulate_keystrokes`.
fn command(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

#[gpui::test]
fn argument_path_is_opened(cx: &mut TestAppContext) {
    let dir = Scratch::new("argument");
    let path = dir.file("notes.md", b"# Titre\n\nUne phrase. Une autre.");
    let session = session();
    let (main, cx) = window(cx, &session, Some(path));
    // L'ouverture attend le fil du lecteur sur l'exécuteur d'arrière-plan.
    cx.run_until_parked();
    wait_for(&session, "le fichier en argument", |s| s.revision >= 1);
    assert!(session.snapshot().has_document);
    assert_eq!(session.document().sentence_count, 3);
    tick(cx);
    main.read_with(cx, |m, _| assert_eq!(m.error(), None));
}

#[gpui::test]
fn dropped_file_is_opened(cx: &mut TestAppContext) {
    let dir = Scratch::new("drop");
    let path = dir.file("depose.txt", "Un texte déposé. Deux phrases.".as_bytes());
    let session = session();
    let (_main, cx) = window(cx, &session, None);
    let at = point(px(400.0), px(250.0));
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths([path].into_iter().collect()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
    wait_for(&session, "le fichier déposé", |s| s.revision >= 1);
    let words: Vec<String> = session
        .document()
        .blocks
        .iter()
        .flat_map(|b| &b.sentences)
        .flat_map(|s| &s.words)
        .map(|w| w.text.clone())
        .collect();
    assert_eq!(words.first().map(String::as_str), Some("Un"));
    assert!(words.iter().any(|w| w == "déposé."));
}

#[gpui::test]
fn dropped_markdown_file_is_opened(cx: &mut TestAppContext) {
    // Une extension que la fenêtre ne connaît pas : le cœur lit le texte
    // à ses octets, comme l'app Mac.
    let dir = Scratch::new("drop-markdown");
    let path = dir.file(
        "notes.markdown",
        b"# Notes\n\nUne phrase en Markdown. Une autre.",
    );
    let session = session();
    let (main, cx) = window(cx, &session, None);
    let at = point(px(400.0), px(250.0));
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths([path].into_iter().collect()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
    wait_for(&session, "le .markdown déposé", |s| s.revision >= 1);
    assert_eq!(session.document().sentence_count, 3);
    main.read_with(cx, |m, _| assert_eq!(m.error(), None));
}

#[gpui::test]
fn refused_file_keeps_current_document(cx: &mut TestAppContext) {
    let dir = Scratch::new("refus");
    let session = session();
    open_text(&session, "Le document courant. Il reste.");
    let revision = session.snapshot().revision;
    let (main, cx) = window(cx, &session, None);

    // Un chemin qui n'existe pas : le cœur le refuse, son erreur est dans
    // la barre.
    let missing = dir.0.join("absent.md");
    let refused = open_path(&session, &missing).expect_err("un fichier absent est refusé");
    assert!(!refused.is_empty());
    main.update(cx, |m, cx| m.open(missing.clone(), cx));
    cx.run_until_parked();
    let line = main.read_with(cx, |m, cx| {
        status_text(&m.reading().read(cx).snapshot().clone(), m.error())
    });
    assert!(line.contains(&refused), "{line}");
    assert!(status(&main, cx).contains(&refused));
    assert_eq!(session.snapshot().revision, revision);

    // Un PDF illisible : le cœur le refuse, son erreur est dans la barre.
    let pdf = dir.file("casse.pdf", b"%PDF-1.7\nceci n'est pas un PDF");
    let core = open_path(&session, &pdf).expect_err("un PDF illisible est refusé");
    assert!(!core.is_empty());
    main.update(cx, |m, cx| m.open(pdf.clone(), cx));
    cx.run_until_parked();
    let line = main.read_with(cx, |m, cx| {
        status_text(&m.reading().read(cx).snapshot().clone(), m.error())
    });
    assert!(line.contains(&core), "{line}");
    assert!(status(&main, cx).contains(&core));
    assert_eq!(session.snapshot().revision, revision);
    assert_eq!(session.document().sentence_count, 2);

    // La prochaine ouverture réussie efface l'erreur.
    let good = dir.file("bon.md", b"Un bon fichier.");
    main.update(cx, |m, cx| m.open(good, cx));
    cx.run_until_parked();
    wait_for(&session, "le bon fichier", |s| s.revision > revision);
    main.read_with(cx, |m, _| assert_eq!(m.error(), None));
    assert!(!status(&main, cx).contains(&core));
}

#[gpui::test]
fn empty_clipboard_keeps_current_document(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Le document courant. Il reste.");
    let revision = session.snapshot().revision;

    let refused = paste_with(&session, || Err::<String, _>("presse-papiers vide"))
        .expect_err("rien à coller");
    assert!(refused.contains("presse-papiers vide"));
    let blank = paste_with(&session, || Ok::<_, String>("  \n ".into())).expect_err("blanc");
    assert!(!blank.is_empty());
    assert_eq!(session.snapshot().revision, revision);

    // Par la fenêtre et ⌘V, avec le presse-papiers injecté.
    let (main, cx) = window(cx, &session, None);
    main.update(cx, |m, _| {
        m.set_clipboard(|| Err("rien dans le presse-papiers".to_string()))
    });
    cx.simulate_keystrokes(&command("v"));
    cx.run_until_parked();
    assert!(status(&main, cx).contains("rien dans le presse-papiers"));
    assert_eq!(session.snapshot().revision, revision);

    // Un vrai texte injecté passe, et l'erreur s'efface.
    main.update(cx, |m, _| {
        m.set_clipboard(|| Ok("Texte collé. Fin.".to_string()))
    });
    cx.simulate_keystrokes(&command("v"));
    cx.run_until_parked();
    wait_for(&session, "le texte collé", |s| s.revision > revision);
    main.read_with(cx, |m, _| assert_eq!(m.error(), None));
}

#[gpui::test]
fn keys_reach_the_reader(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux trois. Quatre cinq six. Sept huit.");
    let (_main, cx) = window(cx, &session, None);
    assert!(!session.snapshot().playing);
    cx.simulate_keystrokes("space");
    wait_for(&session, "la lecture", |s| s.playing);
    cx.simulate_keystrokes("space");
    wait_for(&session, "la pause", |s| !s.playing);
    cx.simulate_keystrokes("right");
    wait_for(&session, "la phrase suivante", |s| s.sentence == 1);
    // Une touche de ponctuation par le caractère produit : + accélère.
    let speed = session.snapshot().speed;
    cx.simulate_keystrokes("shift-=->+");
    wait_for(&session, "plus vite", |s| s.speed > speed + 0.05);
}

#[gpui::test]
fn help_lists_every_key(cx: &mut TestAppContext) {
    for modifier in [Modifier::Command, Modifier::Control] {
        let rows = help::rows(modifier);
        // Chaque ligne d'aide dit vrai : ses touches mènent à une action.
        let mut listed = Vec::new();
        for row in &rows {
            assert!(!row.label.is_empty() && !row.keys.is_empty());
            assert!(!row.presses.is_empty(), "{} : aucune touche", row.keys);
            for press in &row.presses {
                let found = action(press, modifier)
                    .unwrap_or_else(|| panic!("{} : {press:?} ne fait rien", row.keys));
                listed.push(found);
            }
        }
        // Et chaque action que le clavier peut produire a sa ligne.
        for (press, found) in every_binding(modifier) {
            assert!(
                listed.contains(&found),
                "{found:?} ({press:?}) n'a pas de ligne d'aide"
            );
        }
    }

    // `?` montre l'aide, Échap la ferme sans fermer la fenêtre.
    let session = session();
    let (main, cx) = window(cx, &session, None);
    cx.simulate_keystrokes("shift-/->?");
    main.read_with(cx, |m, _| assert!(m.help_visible()));
    cx.simulate_keystrokes("escape");
    main.read_with(cx, |m, _| assert!(!m.help_visible()));
    assert_eq!(cx.windows().len(), 1);
}

/// Toutes les actions que `keymap` donne, sur toutes les touches nommées et
/// tous les caractères ASCII imprimables, seuls, avec Maj ou avec la touche
/// de commande.
fn every_binding(modifier: Modifier) -> Vec<(KeyPress, Action)> {
    let named = [
        "space",
        "escape",
        "home",
        "end",
        "left",
        "right",
        "up",
        "down",
        "enter",
        "tab",
        "backspace",
        "delete",
        "pageup",
        "pagedown",
    ];
    let mut keys: Vec<String> = named.iter().map(|k| k.to_string()).collect();
    keys.extend((0x21u8..0x7f).map(|b| (b as char).to_string()));
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for key in keys {
        for (shift, cmd) in [(false, false), (true, false), (false, true), (true, true)] {
            let press = KeyPress {
                key: key.clone(),
                shift,
                platform: cmd && modifier == Modifier::Command,
                control: cmd && modifier == Modifier::Control,
                ..KeyPress::default()
            };
            if let Some(a) = action(&press, modifier) {
                if seen.insert(format!("{a:?}")) {
                    found.push((press, a));
                }
            }
        }
    }
    assert!(
        found.len() > 15,
        "le clavier donne au moins les actions de la spec"
    );
    found
}

#[gpui::test]
fn transport_buttons_send_commands(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux. Trois quatre. Cinq six. Sept huit.");
    let (main, cx) = window(cx, &session, None);
    tick(cx);
    let focus = main.read_with(cx, |m, _| m.focus_handle().clone());
    let focused = |cx: &mut VisualTestContext| cx.update(|window, _| focus.is_focused(window));
    assert!(focused(cx), "le focus est sur la vue de lecture");

    let click = |cx: &mut VisualTestContext, selector: &'static str| {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} est affiché"));
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
        draw(cx);
    };

    click(cx, "transport-next");
    wait_for(&session, "phrase suivante", |s| s.sentence == 1);
    assert!(focused(cx), "phrase suivante ne prend pas le focus");
    click(cx, "transport-next");
    wait_for(&session, "phrase suivante", |s| s.sentence == 2);
    click(cx, "transport-previous");
    wait_for(&session, "phrase précédente", |s| s.sentence == 1);
    assert!(focused(cx), "phrase précédente ne prend pas le focus");
    tick(cx);
    click(cx, "transport-play");
    wait_for(&session, "lecture", |s| s.playing);
    assert!(focused(cx), "lecture ne prend pas le focus");
    tick(cx);
    click(cx, "transport-play");
    wait_for(&session, "pause", |s| !s.playing);
    // Espace après un clic sur un bouton lit, il ne rappuie pas le bouton.
    cx.simulate_keystrokes("space");
    wait_for(&session, "lecture au clavier", |s| s.playing);
    session.send(Request::Command(Command::Pause));
    wait_for(&session, "pause", |s| !s.playing);
    tick(cx);

    // La vitesse : un menu des vitesses usuelles (0,75 · 1 · 1,25 · 1,5 · 2).
    click(cx, "transport-speed");
    assert!(!focused(cx), "le menu ouvert a le focus, pour ses flèches");
    click(cx, "speed-1.25");
    wait_for(&session, "vitesse 1,25", |s| (s.speed - 1.25).abs() < 0.001);
    draw(cx);
    assert!(
        focused(cx),
        "le menu des vitesses rend le focus à la lecture"
    );
}

#[gpui::test]
fn close_key_pauses_and_closes_the_window(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux trois. Quatre cinq six.");
    let (_main, cx) = window(cx, &session, None);
    cx.simulate_keystrokes("space");
    wait_for(&session, "la lecture", |s| s.playing);
    assert_eq!(cx.windows().len(), 1);
    cx.simulate_keystrokes(&command("w"));
    cx.run_until_parked();
    wait_for(&session, "la pause à la fermeture", |s| !s.playing);
    assert!(cx.windows().is_empty(), "⌘W ferme la fenêtre");
}

#[gpui::test]
fn escape_closes_the_window_when_nothing_is_open(cx: &mut TestAppContext) {
    let session = session();
    let (_main, cx) = window(cx, &session, None);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.windows().is_empty(), "Échap sans rien d'ouvert ferme");
}

#[gpui::test]
fn copy_puts_the_sentence_on_the_clipboard(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux trois. Quatre cinq six.");
    session.send(Request::Command(Command::Sentences(1)));
    wait_for(&session, "la deuxième phrase", |s| s.sentence == 1);
    let (main, cx) = window(cx, &session, None);
    tick(cx);
    cx.simulate_keystrokes(&command("c"));
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("Quatre cinq six."));
    assert!(!status(&main, cx).is_empty(), "la copie se dit");
}

#[gpui::test]
fn open_key_prompts_for_a_file(cx: &mut TestAppContext) {
    let dir = Scratch::new("prompt");
    let path = dir.file("choisi.md", b"Choisi dans la boite. Fin.");
    let session = session();
    let (_main, cx) = window(cx, &session, None);
    cx.simulate_keystrokes(&command("o"));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_paths(), "⌘O ouvre la boîte du système");
    let chosen = path.clone();
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![chosen])
    });
    cx.run_until_parked();
    wait_for(&session, "le fichier choisi", |s| s.revision >= 1);
    assert_eq!(session.document().sentence_count, 2);
}

#[gpui::test]
fn idle_window_does_not_redraw(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux trois. Quatre cinq six.");
    let (main, cx) = window(cx, &session, None);
    tick(cx);
    tick(cx);
    // Chaque `notify` de la fenêtre ou de la vue de lecture demande une
    // image : au repos, cinq secondes d'images dessinées n'en demandent
    // aucune.
    let notified = Rc::new(std::cell::Cell::new(0usize));
    let reading = main.read_with(cx, |m, _| m.reading().clone());
    cx.update(|_, cx| {
        let count = Rc::clone(&notified);
        cx.observe(&main, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&reading, move |_, _| count.set(count.get() + 1))
            .detach();
    });
    let renders = main.read_with(cx, |m, _| m.renders());
    for _ in 0..20 {
        tick(cx);
    }
    assert_eq!(notified.get(), 0, "au repos, rien ne demande d'image");
    assert!(main.read_with(cx, |m, _| m.renders()) > renders);
    // Contrôle de la mesure : un changement du lecteur, lui, en demande.
    session.send(Request::Command(Command::Sentences(1)));
    wait_for(&session, "la phrase suivante", |s| s.sentence == 1);
    tick(cx);
    assert!(notified.get() > 0, "la mesure voit les changements");
}

#[test]
fn punctuation_keys_follow_the_layout() {
    let stroke = |key: &str, shift: bool, key_char: Option<&str>| Keystroke {
        modifiers: Modifiers {
            shift,
            ..Modifiers::default()
        },
        key: key.into(),
        key_char: key_char.map(str::to_string),
    };
    let mac = Modifier::Command;
    // AZERTY : ? est Maj+, ; + est Maj+= ; / est Maj+: .
    assert_eq!(
        action(&key_press(&stroke(",", true, Some("?"))), mac),
        Some(Action::ShowHelp)
    );
    assert_eq!(
        action(&key_press(&stroke("=", true, Some("+"))), mac),
        Some(Action::Reader(Command::ChangeSpeed(0.1)))
    );
    assert_eq!(
        action(&key_press(&stroke(":", true, Some("/"))), mac),
        Some(Action::OpenSearch)
    );
    assert_eq!(
        action(&key_press(&stroke("-", false, Some("-"))), mac),
        Some(Action::Reader(Command::ChangeSpeed(-0.1)))
    );
    // US : ? est Maj+/.
    assert_eq!(
        action(&key_press(&stroke("/", true, Some("?"))), mac),
        Some(Action::ShowHelp)
    );
    // Les lettres gardent leur touche et Maj : ⇧N, résultat suivant.
    assert_eq!(
        action(&key_press(&stroke("n", true, Some("N"))), mac),
        Some(Action::Reader(Command::SearchAdvance(1)))
    );
    // Une touche nommée garde son nom.
    assert_eq!(
        action(&key_press(&stroke("space", false, Some(" "))), mac),
        Some(Action::Reader(Command::TogglePlay))
    );
    // La commande et sa touche, sans caractère produit : ⌘, ouvre les réglages.
    let settings = Keystroke {
        modifiers: Modifiers {
            platform: true,
            ..Modifiers::default()
        },
        key: ",".into(),
        key_char: None,
    };
    assert_eq!(
        action(&key_press(&settings), mac),
        Some(Action::OpenSettings)
    );
    let expected = if cfg!(target_os = "macos") {
        Modifier::Command
    } else {
        Modifier::Control
    };
    assert_eq!(inputs::command_modifier(), expected);
}

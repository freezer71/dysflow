//! La modification du texte, en headless : un vrai lecteur muet
//! (`silent_reader`) sur son fil, la plateforme de test de GPUI. Aucune
//! fenêtre réelle, aucun son.
//!
//! Les cas de `macos/Tests/EditingTests.swift` et de
//! `PanelKeyRouterTests.testCommandEnterFinishesEditing`, plus ceux du plan.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::editable::{editable_source, resume_sentence};
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::view::Snapshot;
use dysflow_app::keymap::{action, Action, KeyPress};
use dysflow_app::session::Session;
use dysflow_app::window::{MainWindow, Panel};
use gpui::{px, size, Entity, Modifiers, TestAppContext, VisualTestContext};

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

/// Une barrière : l'instantané calculé sur le fil du lecteur, une fois
/// traité tout ce qui le précède dans sa file.
fn settle(session: &Session) -> Snapshot {
    session
        .ask(|r| dysflow::reader::view::snapshot(r))
        .expect("le lecteur répond")
}

fn open_text(session: &Session, markdown: &str) {
    session.send(Request::OpenText {
        markdown: markdown.into(),
        source: "test".into(),
    });
    wait_for(session, "le texte", |s| s.has_document && s.revision >= 1);
}

fn seek(session: &Session, sentence: usize) {
    session.send(Request::Command(Command::SeekTo(sentence)));
    wait_for(session, "la phrase", |s| s.sentence == sentence);
}

/// Le texte ouvert dans le lecteur, tel quel.
fn opened_text(session: &Session) -> Option<String> {
    session
        .ask(|r| r.text_markdown().map(str::to_string))
        .flatten()
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    draw(cx);
}

fn command(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
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
        let main = MainWindow::build(shared, None::<PathBuf>, window, cx);
        *out.borrow_mut() = Some(main.clone());
        gpui_component::Root::new(main, window, cx)
    });
    cx.simulate_resize(size(px(900.0), px(600.0)));
    cx.run_until_parked();
    draw(cx);
    let main = slot.borrow().clone().expect("la fenêtre principale");
    tick(cx);
    (main, cx)
}

fn panel(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Option<Panel> {
    main.read_with(cx, |m, _| m.panel())
}

fn editing(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    panel(main, cx) == Some(Panel::Edit)
}

/// `e`, puis l'attente du texte demandé au lecteur (exécuteur
/// d'arrière-plan) ; l'éditeur dessiné.
fn open_editor(main: &Entity<MainWindow>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("e");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !editing(main, cx) {
        assert!(Instant::now() < deadline, "l'éditeur ne s'ouvre pas");
        cx.run_until_parked();
        std::thread::yield_now();
    }
    draw(cx);
}

fn draft(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> String {
    main.read_with(cx, |m, cx| m.editor().read(cx).text(cx))
}

fn set_draft(main: &Entity<MainWindow>, cx: &mut VisualTestContext, text: &str) {
    let editor = main.read_with(cx, |m, _| m.editor().clone());
    cx.update(|window, cx| editor.update(cx, |e, cx| e.set_text(text, window, cx)));
}

fn reading_focused(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    let focus = main.read_with(cx, |m, _| m.focus_handle().clone());
    cx.update(|window, _| focus.is_focused(window))
}

fn editor_focused(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    let editor = main.read_with(cx, |m, _| m.editor().clone());
    cx.update(|window, cx| editor.read(cx).focus_handle(cx).is_focused(window))
}

fn current_sentence_text(session: &Session) -> String {
    let s = session.snapshot();
    session
        .document()
        .blocks
        .iter()
        .flat_map(|b| &b.sentences)
        .find(|x| x.id == s.sentence)
        .map(|x| {
            x.words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// `testTheEKeyEdits` : `e` modifie, et l'aide le dit.
#[test]
fn the_e_key_edits() {
    let e = KeyPress {
        key: "e".into(),
        ..KeyPress::default()
    };
    assert_eq!(
        action(&e, dysflow_app::inputs::command_modifier()),
        Some(Action::Edit)
    );
    let label = dysflow::tr!("Edit the text", "Modifier le texte");
    assert!(
        dysflow_app::help::rows(dysflow_app::inputs::command_modifier())
            .iter()
            .any(|r| r.label == label)
    );
}

/// `testEditingStartsFromTheOpenedText`, et le plan : `e` ouvre l'éditeur
/// sur le Markdown ouvert, tel quel, le focus dans le champ.
#[gpui::test]
fn e_opens_editor_with_markdown(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "# Titre\n\nUn. Deux.");
    let (main, cx) = window(cx, &session);
    assert!(!editing(&main, cx));
    open_editor(&main, cx);
    assert_eq!(draft(&main, cx), "# Titre\n\nUn. Deux.");
    assert!(editor_focused(&main, cx), "le champ a le focus");
    assert!(!reading_focused(&main, cx));
    assert!(
        cx.debug_bounds("editor-text").is_some(),
        "le champ est dessiné"
    );
    assert!(
        cx.debug_bounds("editor-bar").is_some(),
        "la barre d'édition est dessinée"
    );
    assert_eq!(
        main.read_with(cx, |m, cx| m.editor().read(cx).source().to_string()),
        "test",
        "la source du document ouvert"
    );
}

/// Sans document, `e` n'ouvre rien.
#[gpui::test]
fn e_without_a_document_opens_nothing(cx: &mut TestAppContext) {
    let session = session();
    let (main, cx) = window(cx, &session);
    cx.simulate_keystrokes("e");
    cx.run_until_parked();
    tick(cx);
    assert!(!editing(&main, cx));
}

/// Ouvrir l'éditeur met la lecture en pause ; validé sans changement, la
/// voix repart (`beginEditing` / `finishEditing` de l'app Mac).
#[gpui::test]
fn editing_pauses_the_reading_and_resumes_it(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un deux trois. Quatre cinq six. Sept huit neuf.");
    let (main, cx) = window(cx, &session);
    cx.simulate_keystrokes("space");
    wait_for(&session, "la lecture", |s| s.playing);
    open_editor(&main, cx);
    wait_for(&session, "la pause", |s| !s.playing);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    wait_for(&session, "la reprise", |s| s.playing);
    assert_eq!(session.snapshot().revision, 1, "rien ne s'est rechargé");
}

/// Routage : les lettres, Espace, Entrée, `/`, `?` s'écrivent dans
/// l'éditeur ; aucune action du lecteur ne part.
#[gpui::test]
fn typing_in_the_editor_stays_in_the_editor(cx: &mut TestAppContext) {
    let session = session();
    open_text(
        &session,
        "# Premier\n\nUn deux. Trois quatre.\n\n# Second\n\nCinq six.",
    );
    let (main, cx) = window(cx, &session);
    seek(&session, 1);
    let before = settle(&session);
    open_editor(&main, cx);
    let origin = draft(&main, cx);
    cx.simulate_keystrokes("n e t space / ? enter p");
    cx.run_until_parked();
    draw(cx);
    assert_eq!(draft(&main, cx), format!("net /?\np{origin}"));
    let after = settle(&session);
    assert_eq!(after.section, before.section, "la section n'a pas bougé");
    assert_eq!(after.sentence, before.sentence, "la phrase n'a pas bougé");
    assert!(!after.playing, "Espace n'a rien lancé");
    assert!(after.search.is_none());
    main.read_with(cx, |m, _| {
        assert!(!m.search_visible(), "/ n'a pas ouvert la recherche");
        assert!(!m.help_visible(), "? n'a pas ouvert l'aide");
        assert_eq!(m.panel(), Some(Panel::Edit), "t, e : toujours l'éditeur");
    });
    assert!(editor_focused(&main, cx));
}

/// `testTheEditedTextReplacesTheDocumentAtTheSameSentence`.
#[gpui::test]
fn the_edited_text_replaces_the_document_at_the_same_sentence(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux. Trois. Quatre.");
    let (main, cx) = window(cx, &session);
    seek(&session, 2);
    tick(cx);
    open_editor(&main, cx);
    set_draft(&main, cx, "Un. Deux. Trois corrigée. Quatre.");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    wait_for(&session, "le texte modifié", |s| {
        s.revision == 2 && s.sentence == 2
    });
    settle(&session);
    assert_eq!(
        opened_text(&session).as_deref(),
        Some("Un. Deux. Trois corrigée. Quatre.")
    );
    assert!(current_sentence_text(&session).contains("corrigée"));
}

/// `testAShorterTextResumesAtItsLastSentence`.
#[gpui::test]
fn a_shorter_text_resumes_at_its_last_sentence(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux. Trois. Quatre.");
    let (main, cx) = window(cx, &session);
    seek(&session, 3);
    tick(cx);
    open_editor(&main, cx);
    set_draft(&main, cx, "Un. Deux.");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    wait_for(&session, "le texte raccourci", |s| s.revision == 2);
    let s = settle(&session);
    assert_eq!(s.sentence, 1);
}

/// Review Focus 5 : la moitié du texte supprimée, dont la phrase en cours ;
/// « Terminé » : la phrase en cours vaut `resume_sentence`, le lecteur
/// répond toujours, la source est la même.
#[gpui::test]
fn validating_reopens_and_resumes_at_clamped_sentence(cx: &mut TestAppContext) {
    let session = session();
    let first = "Un deux. Trois quatre. Cinq six.";
    open_text(
        &session,
        &format!("{first}\n\nSept huit. Neuf dix. Onze douze."),
    );
    let (main, cx) = window(cx, &session);
    seek(&session, 4);
    tick(cx);
    open_editor(&main, cx);
    set_draft(&main, cx, first);
    draw(cx);
    let done = cx.debug_bounds("editor-done").expect("le bouton Terminé");
    cx.simulate_click(done.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    wait_for(&session, "le texte réduit", |s| s.revision == 2);
    let s = settle(&session);
    let count = session.document().sentence_count;
    assert_eq!(count, 3);
    assert_eq!(Some(s.sentence), resume_sentence(4, count));
    assert_eq!(s.sentence, 2);
    assert_eq!(opened_text(&session).as_deref(), Some(first));
    assert_eq!(
        session.ask(|r| editable_source(r)).flatten().as_deref(),
        Some("test"),
        "même source"
    );
    assert!(reading_focused(&main, cx), "le focus revient à la lecture");
}

/// Un texte vidé (ou seulement des espaces) : rien à lire, sans panique ni
/// saut hors du texte.
#[gpui::test]
fn validating_empty_text_shows_nothing_to_read(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux. Trois.");
    let (main, cx) = window(cx, &session);
    seek(&session, 2);
    tick(cx);
    open_editor(&main, cx);
    set_draft(&main, cx, "  \n\n  ");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    wait_for(&session, "le document fermé", |s| !s.has_document);
    let s = settle(&session);
    assert!(!s.has_document);
    assert!(!s.playing);
    assert_eq!(opened_text(&session), None);
    tick(cx);
    assert!(!editing(&main, cx));
    assert!(cx.debug_bounds("invitation").is_some(), "« Rien à lire »");
    assert!(reading_focused(&main, cx));
}

/// `ReaderAction.escape` : « la saisie se valide ». Échap ferme l'éditeur,
/// rend le focus à la lecture, laisse la fenêtre ouverte, et ne s'écrit pas
/// dans le texte.
#[gpui::test]
fn escape_validates_like_the_mac(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux.");
    let (main, cx) = window(cx, &session);
    open_editor(&main, cx);
    cx.simulate_keystrokes("x");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    assert!(reading_focused(&main, cx));
    assert_eq!(cx.windows().len(), 1, "la fenêtre reste ouverte");
    wait_for(&session, "le texte modifié", |s| s.revision == 2);
    settle(&session);
    assert_eq!(opened_text(&session).as_deref(), Some("xUn. Deux."));
    // Échap depuis la lecture, ensuite : la fenêtre se ferme, comme avant.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
}

/// `testEscapeValidatesAndKeepsThePanel`.
#[gpui::test]
fn escape_validates_and_keeps_the_panel(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux.");
    let (main, cx) = window(cx, &session);
    open_editor(&main, cx);
    set_draft(&main, cx, "Un. Trois.");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    assert_eq!(cx.windows().len(), 1);
    wait_for(&session, "le texte modifié", |s| s.revision == 2);
}

/// `testNothingReloadsWhenNothingChanged` : validé tel quel, puis modifié
/// et annulé : rien ne se recharge.
#[gpui::test]
fn nothing_reloads_when_nothing_changed(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux.");
    let (main, cx) = window(cx, &session);
    open_editor(&main, cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    open_editor(&main, cx);
    set_draft(&main, cx, "Autre chose.");
    draw(cx);
    let cancel = cx.debug_bounds("editor-cancel").expect("le bouton Annuler");
    cx.simulate_click(cancel.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    assert!(reading_focused(&main, cx));
    let s = settle(&session);
    assert_eq!(s.revision, 1);
    assert_eq!(opened_text(&session).as_deref(), Some("Un. Deux."));
}

/// `testCommandEnterFinishesEditing` : ⌘↩ valide, sans aller à la ligne.
#[gpui::test]
fn command_enter_validates(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux.");
    let (main, cx) = window(cx, &session);
    open_editor(&main, cx);
    cx.simulate_keystrokes("x");
    cx.simulate_keystrokes(&command("enter"));
    cx.run_until_parked();
    assert!(!editing(&main, cx));
    wait_for(&session, "le texte modifié", |s| s.revision == 2);
    settle(&session);
    assert_eq!(opened_text(&session).as_deref(), Some("xUn. Deux."));
}

/// L'éditeur ouvert, au repos : ni la fenêtre ni l'éditeur ne demandent
/// d'image (le curseur qui clignote est celui du champ).
#[gpui::test]
fn an_open_editor_stays_quiet_at_rest(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, "Un. Deux.");
    let (main, cx) = window(cx, &session);
    open_editor(&main, cx);
    tick(cx);
    tick(cx);
    let notified = Rc::new(std::cell::Cell::new(0usize));
    let (reading, editor) = main.read_with(cx, |m, _| (m.reading().clone(), m.editor().clone()));
    cx.update(|_, cx| {
        let count = Rc::clone(&notified);
        cx.observe(&main, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&reading, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&editor, move |_, _| count.set(count.get() + 1))
            .detach();
    });
    for _ in 0..20 {
        tick(cx);
    }
    assert_eq!(notified.get(), 0, "au repos, rien ne demande d'image");
    // Contrôle de la mesure : un changement du lecteur, lui, en demande.
    session.send(Request::Command(Command::Sentences(1)));
    wait_for(&session, "la phrase suivante", |s| s.sentence == 1);
    tick(cx);
    assert!(notified.get() > 0, "la mesure voit les changements");
}

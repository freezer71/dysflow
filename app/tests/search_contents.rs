//! La barre de recherche et le sommaire, en headless : un vrai lecteur muet
//! (`silent_reader`) sur son fil, la plateforme de test de GPUI. Aucune
//! fenêtre réelle, aucun son.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::view::Snapshot;
use dysflow_app::contents::{ContentsEvent, ContentsRow};
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

/// La fenêtre principale dans une fenêtre de test de 900 × 600, sous la
/// racine de gpui-component, comme dans `run`.
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

fn reading_focused(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> bool {
    let focus = main.read_with(cx, |m, _| m.focus_handle().clone());
    cx.update(|window, _| focus.is_focused(window))
}

fn query(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> String {
    main.read_with(cx, |m, cx| m.search().read(cx).query(cx))
}

fn result(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Option<String> {
    main.read_with(cx, |m, cx| m.search().read(cx).result())
}

fn rows(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Vec<ContentsRow> {
    main.read_with(cx, |m, cx| m.contents().read(cx).rows(cx).to_vec())
}

/// `debug_bounds` veut un nom qui vive toujours.
fn selector(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

fn of(position: usize, count: usize) -> String {
    dysflow::tr!(en = "{} of {}", fr = "{} sur {}", position, count)
}

const TEXT: &str = "Un deux trois. Quatre cinq. Deux six. Sept deux. Huit neuf.";

/// Ouvre la recherche, tape `typed` touche par touche et valide.
fn search_for(cx: &mut VisualTestContext, typed: &str) {
    cx.simulate_keystrokes("/");
    draw(cx);
    let keys: Vec<String> = typed
        .chars()
        .map(|c| {
            if c == ' ' {
                "space".into()
            } else {
                c.to_string()
            }
        })
        .collect();
    cx.simulate_keystrokes(&keys.join(" "));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn typing_searches_and_counts(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, TEXT);
    let (main, cx) = window(cx, &session);

    cx.simulate_keystrokes("/");
    draw(cx);
    main.read_with(cx, |m, _| assert!(m.search_visible()));
    assert!(!reading_focused(&main, cx), "le champ a le focus");
    cx.simulate_keystrokes("d e u x");
    assert_eq!(query(&main, cx), "deux");
    cx.simulate_keystrokes("enter");
    wait_for(&session, "la recherche", |s| {
        s.search
            .as_ref()
            .is_some_and(|q| q.query == "deux" && q.position == 1 && q.count == 3)
    });
    tick(cx);
    assert_eq!(result(&main, cx), Some(of(1, 3)));
    // Comme l'app Mac : Entrée valide et rend la main à la lecture.
    assert!(reading_focused(&main, cx));

    // Une requête sans résultat le dit, sans déplacer la lecture.
    let sentence = session.snapshot().sentence;
    search_for(cx, "zzz");
    let deadline = Instant::now() + Duration::from_secs(5);
    while result(&main, cx).as_deref() != Some(dysflow::tr!("No match", "Aucun résultat")) {
        assert!(Instant::now() < deadline, "aucun résultat : jamais affiché");
        tick(cx);
    }
    assert_eq!(session.snapshot().search, None);
    assert_eq!(session.snapshot().sentence, sentence);
}

#[gpui::test]
fn n_and_p_advance(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, TEXT);
    let (main, cx) = window(cx, &session);
    search_for(cx, "deux");
    wait_for(&session, "la recherche", |s| {
        s.search.as_ref().is_some_and(|q| q.position == 1)
    });
    tick(cx);
    assert!(reading_focused(&main, cx));

    let at = |n: usize| move |s: &Snapshot| s.search.as_ref().is_some_and(|q| q.position == n);
    cx.simulate_keystrokes("shift-n");
    wait_for(&session, "N : le résultat 2", at(2));
    cx.simulate_keystrokes("shift-n");
    wait_for(&session, "N : le résultat 3", at(3));
    cx.simulate_keystrokes("shift-p");
    wait_for(&session, "P : le résultat 2", at(2));
    tick(cx);
    assert_eq!(result(&main, cx), Some(of(2, 3)));
    // La lecture suit le résultat.
    assert_eq!(session.snapshot().sentence, 2);

    // Dans le champ, la même requête : Entrée avance, Maj+Entrée recule.
    cx.simulate_keystrokes("/");
    draw(cx);
    assert_eq!(
        query(&main, cx),
        "deux",
        "le champ rouvert garde la requête"
    );
    cx.simulate_keystrokes("enter");
    wait_for(&session, "Entrée : le résultat 3", at(3));
    cx.simulate_keystrokes("/");
    draw(cx);
    cx.simulate_keystrokes("shift-enter");
    wait_for(&session, "Maj+Entrée : le résultat 2", at(2));
    tick(cx);
    assert_eq!(result(&main, cx), Some(of(2, 3)));
}

#[gpui::test]
fn escape_cancels_search(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, TEXT);
    let (main, cx) = window(cx, &session);
    search_for(cx, "deux");
    wait_for(&session, "la recherche", |s| s.search.is_some());

    // Échap dans le champ : la recherche s'annule, la barre se ferme, le
    // focus revient à la lecture.
    cx.simulate_keystrokes("/");
    draw(cx);
    assert!(!reading_focused(&main, cx));
    cx.simulate_keystrokes("escape");
    wait_for(&session, "l'annulation", |s| s.search.is_none());
    tick(cx);
    main.read_with(cx, |m, _| assert!(!m.search_visible()));
    assert!(reading_focused(&main, cx));
    assert_eq!(cx.windows().len(), 1, "Échap ne ferme pas la fenêtre");

    // Rouverte, la barre repart vide.
    cx.simulate_keystrokes("/");
    draw(cx);
    assert_eq!(query(&main, cx), "");
    assert_eq!(result(&main, cx), None);

    // Échap depuis la lecture, barre ouverte et recherche en cours : même
    // chose.
    cx.simulate_keystrokes("d e u x enter");
    wait_for(&session, "la recherche", |s| s.search.is_some());
    tick(cx);
    assert!(reading_focused(&main, cx));
    cx.simulate_keystrokes("escape");
    wait_for(&session, "l'annulation", |s| s.search.is_none());
    tick(cx);
    main.read_with(cx, |m, _| assert!(!m.search_visible()));
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
fn letters_typed_in_the_field_stay_in_the_field(cx: &mut TestAppContext) {
    let session = session();
    open_text(
        &session,
        "# Un\n\nPremière phrase. Deuxième phrase.\n\n# Deux\n\nTroisième phrase.",
    );
    let (main, cx) = window(cx, &session);
    let before = session.snapshot();
    cx.simulate_keystrokes("/");
    draw(cx);
    // n (section suivante), p, t (sommaire), e (édition), f (suivre), Espace
    // (lecture), Maj+N (résultat suivant) : tout s'écrit dans la requête.
    cx.simulate_keystrokes("n p t e f space shift-n");
    tick(cx);
    assert_eq!(query(&main, cx), "nptef N");
    let after = session.snapshot();
    assert_eq!(after.section, before.section, "n ne change pas de section");
    assert_eq!(after.sentence, before.sentence);
    assert!(!after.playing, "Espace ne lance pas la lecture");
    assert_eq!(after.search, None, "rien n'est cherché avant Entrée");
    main.read_with(cx, |m, _| {
        assert_eq!(m.panel(), None, "t n'ouvre pas le sommaire");
        assert!(m.search_visible());
    });
}

const SECTIONS: &str = "# Premier\n\nUne phrase. Deux phrases.\n\n## Sous-partie\n\nTrois.\n\n# Second\n\nQuatre. Cinq.";

#[gpui::test]
fn contents_lists_sections_and_marks_current(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, SECTIONS);
    let doc = session.document();
    let sub = doc
        .sections
        .iter()
        .find(|s| s.title == "Sous-partie")
        .expect("la sous-partie");
    let target = sub.first_sentence.expect("une phrase");
    session.send(Request::Command(Command::SeekTo(target)));
    wait_for(&session, "la sous-partie", |s| s.section == Some(sub.id));
    let (main, cx) = window(cx, &session);

    cx.simulate_keystrokes("t");
    draw(cx);
    main.read_with(cx, |m, _| assert_eq!(m.panel(), Some(Panel::Contents)));
    let shown = rows(&main, cx);
    let titles: Vec<&str> = shown.iter().map(|r| r.title.as_str()).collect();
    // La section implicite, sans phrase avant le premier titre, n'est pas
    // un titre : elle n'y est pas.
    assert_eq!(titles, ["Premier", "Sous-partie", "Second"]);
    assert_eq!(shown.iter().map(|r| r.level).collect::<Vec<_>>(), [1, 2, 1]);
    let current: Vec<&str> = shown
        .iter()
        .filter(|r| r.current)
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(current, ["Sous-partie"]);

    // Le repère est dessiné sur la ligne de la section lue, et le retrait
    // suit le niveau.
    let marker = cx.debug_bounds("contents-marker").expect("le repère");
    let row = cx.debug_bounds("contents-row-1").expect("la ligne 1");
    assert!(
        row.contains(&marker.center()),
        "le repère est sur la ligne lue"
    );
    let first = cx.debug_bounds("contents-title-0").expect("titre 0");
    let second = cx.debug_bounds("contents-title-1").expect("titre 1");
    assert!(
        second.origin.x > first.origin.x,
        "un titre de niveau 2 est en retrait"
    );

    // La lecture avance : le repère suit.
    let second_section = shown[2].clone();
    session.send(Request::Command(Command::SeekTo(
        second_section.target.expect("une phrase"),
    )));
    wait_for(&session, "la deuxième section", |s| {
        s.section == Some(second_section.section)
    });
    tick(cx);
    let current: Vec<String> = rows(&main, cx)
        .into_iter()
        .filter(|r| r.current)
        .map(|r| r.title)
        .collect();
    assert_eq!(current, ["Second"]);
    let marker = cx.debug_bounds("contents-marker").expect("le repère");
    let row = cx.debug_bounds("contents-row-2").expect("la ligne 2");
    assert!(row.contains(&marker.center()));

    // t referme le sommaire.
    cx.simulate_keystrokes("t");
    draw(cx);
    main.read_with(cx, |m, _| assert_eq!(m.panel(), None));
    assert!(reading_focused(&main, cx));
}

#[gpui::test]
fn choosing_a_section_seeks(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, SECTIONS);
    let (main, cx) = window(cx, &session);
    cx.simulate_keystrokes("t");
    draw(cx);
    let shown = rows(&main, cx);
    assert!(!reading_focused(&main, cx), "le sommaire ouvert a le focus");

    // Au clavier : la section lue est choisie d'abord, ↓ ↓ puis Entrée.
    cx.simulate_keystrokes("down down");
    draw(cx);
    cx.simulate_keystrokes("enter");
    let second = shown[2].target.expect("une phrase");
    wait_for(&session, "Entrée : le second titre", |s| {
        s.sentence == second
    });
    tick(cx);
    assert!(
        reading_focused(&main, cx),
        "après un choix, le focus revient"
    );
    main.read_with(cx, |m, _| assert_eq!(m.panel(), Some(Panel::Contents)));

    // À la souris.
    let row = cx.debug_bounds("contents-row-1").expect("la ligne 1");
    cx.simulate_click(row.center(), Modifiers::none());
    let sub = shown[1].target.expect("une phrase");
    wait_for(&session, "clic : la sous-partie", |s| s.sentence == sub);
    tick(cx);
    assert!(reading_focused(&main, cx));

    // t sur le sommaire ouvert le ferme ; rouvert, il a le focus ; Échap
    // dans la liste le ferme sans fermer la fenêtre.
    cx.simulate_keystrokes("t");
    draw(cx);
    main.read_with(cx, |m, _| assert_eq!(m.panel(), None));
    cx.simulate_keystrokes("t");
    draw(cx);
    assert!(!reading_focused(&main, cx));
    cx.simulate_keystrokes("escape");
    draw(cx);
    main.read_with(cx, |m, _| assert_eq!(m.panel(), None));
    assert!(reading_focused(&main, cx));
    assert_eq!(cx.windows().len(), 1);
}

#[gpui::test]
fn section_without_sentence_is_disabled(cx: &mut TestAppContext) {
    let session = session();
    // Un titre fait d'un emoji seul n'a aucune phrase à lire.
    open_text(
        &session,
        "# Début\n\nUne phrase.\n\n# 🙂\n\n# Fin\n\nDernière phrase.",
    );
    let doc = session.document();
    assert!(
        doc.sections
            .iter()
            .any(|s| s.level > 0 && s.first_sentence.is_none()),
        "le document a un titre sans phrase : {:?}",
        doc.sections
    );
    let last = doc.sentence_count - 1;
    session.send(Request::Command(Command::SeekTo(last)));
    wait_for(&session, "la dernière phrase", |s| s.sentence == last);
    let (main, cx) = window(cx, &session);

    let chosen = Rc::new(std::cell::Cell::new(0usize));
    let contents = main.read_with(cx, |m, _| m.contents().clone());
    cx.update(|_, cx| {
        let count = Rc::clone(&chosen);
        cx.subscribe(&contents, move |_, event: &ContentsEvent, _| {
            if matches!(event, ContentsEvent::Chosen(_)) {
                count.set(count.get() + 1);
            }
        })
        .detach();
    });

    cx.simulate_keystrokes("t");
    draw(cx);
    let shown = rows(&main, cx);
    let blank = shown
        .iter()
        .position(|r| r.target.is_none())
        .expect("le titre sans phrase est listé");
    assert!(!shown[blank].enabled(), "affiché comme désactivé");
    assert!(shown
        .iter()
        .filter(|r| r.target.is_some())
        .all(|r| r.enabled()));
    assert!(
        cx.debug_bounds(selector(format!("contents-disabled-{blank}")))
            .is_some(),
        "la ligne est dessinée désactivée"
    );

    // Un clic n'y mène nulle part.
    let row = cx
        .debug_bounds(selector(format!("contents-row-{blank}")))
        .expect("la ligne");
    cx.simulate_click(row.center(), Modifiers::none());
    tick(cx);
    // Entrée non plus : la sélection est posée sur ce titre.
    cx.update(|window, cx| contents.update(cx, |c, cx| c.select(blank, window, cx)));
    let focus = main.read_with(cx, |m, cx| m.contents().read(cx).focus_handle(cx));
    cx.update(|window, cx| focus.focus(window, cx));
    draw(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(chosen.get(), 0, "aucun choix sur un titre désactivé");
    assert_eq!(
        session.snapshot().sentence,
        last,
        "la lecture n'a pas bougé"
    );
}

#[gpui::test]
fn open_search_and_contents_stay_quiet_at_rest(cx: &mut TestAppContext) {
    let session = session();
    open_text(&session, SECTIONS);
    let (main, cx) = window(cx, &session);
    search_for(cx, "phrase");
    wait_for(&session, "la recherche", |s| s.search.is_some());
    // Le sommaire ouvert, puis le focus rendu à la lecture (un clic dans la
    // page) : ni le champ ni la liste n'ont le focus.
    cx.simulate_keystrokes("t");
    draw(cx);
    let reading = main.read_with(cx, |m, _| m.reading().clone());
    let focus = main.read_with(cx, |m, _| m.focus_handle().clone());
    cx.update(|window, cx| focus.focus(window, cx));
    tick(cx);
    tick(cx);
    assert_eq!(cx.windows().len(), 1);
    main.read_with(cx, |m, _| assert!(m.search_visible()));
    main.read_with(cx, |m, _| assert_eq!(m.panel(), Some(Panel::Contents)));

    // Barre de recherche et sommaire ouverts, rien ne bouge : cinq
    // secondes d'images dessinées sans aucune demande d'image.
    let notified = Rc::new(std::cell::Cell::new(0usize));
    let (search, contents) = main.read_with(cx, |m, _| (m.search().clone(), m.contents().clone()));
    cx.update(|_, cx| {
        let count = Rc::clone(&notified);
        cx.observe(&main, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&reading, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&search, move |_, _| count.set(count.get() + 1))
            .detach();
        let count = Rc::clone(&notified);
        cx.observe(&contents, move |_, _| count.set(count.get() + 1))
            .detach();
    });
    for _ in 0..20 {
        tick(cx);
    }
    assert_eq!(notified.get(), 0, "au repos, rien ne demande d'image");
    // Contrôle de la mesure : un changement du lecteur, lui, en demande.
    let before = session.snapshot().sentence;
    session.send(Request::Command(Command::Sentences(1)));
    wait_for(&session, "la phrase suivante", |s| s.sentence == before + 1);
    tick(cx);
    assert!(notified.get() > 0, "la mesure voit les changements");
}

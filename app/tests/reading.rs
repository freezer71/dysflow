//! La vue de lecture, en headless : un vrai lecteur muet (`silent_reader`)
//! sur son fil, la plateforme de test de GPUI, une horloge simulée pour la
//! boucle de rafraîchissement. Aucune fenêtre réelle, aucun son.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::reader::build::silent_reader;
use dysflow::reader::command::Command;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::view::{BlockView, DocView, SentenceView, Snapshot, WordView};
use dysflow_app::reading::ReadingView;
use dysflow_app::session::Session;
use gpui::{
    point, px, size, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext,
};

/// Un Markdown de `n` paragraphes d'une phrase chacun, assez pour défiler.
fn paragraphs(n: usize) -> String {
    (0..n)
        .map(|i| format!("Phrase {i} avec quelques mots."))
        .collect::<Vec<_>>()
        .join("\n\n")
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

/// Un rendu complet de la fenêtre de test.
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// Laisse passer un tour de la boucle de rafraîchissement (horloge simulée),
/// puis redessine.
fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    draw(cx);
}

/// Le texte ouvert dans un lecteur muet, la vue dans une fenêtre de test de
/// 600 × 300.
fn open<'a>(
    cx: &'a mut TestAppContext,
    markdown: &str,
) -> (Arc<Session>, Entity<ReadingView>, &'a mut VisualTestContext) {
    let session = Arc::new(Session::new(ReaderHandle::spawn(silent_reader)));
    session.send(Request::OpenText {
        markdown: markdown.into(),
        source: "test".into(),
    });
    wait_for(&session, "le document", |s| {
        s.has_document && s.revision >= 1
    });
    let shared = Arc::clone(&session);
    let (view, cx) = cx.add_window_view(move |_window, cx| ReadingView::new(shared, cx));
    cx.simulate_resize(size(px(600.0), px(300.0)));
    draw(cx);
    (session, view, cx)
}

/// L'index global (`WordView.index`) du mot `word` de la phrase `sentence`.
fn index_of(session: &Session, (sentence, word): (usize, usize)) -> usize {
    session
        .document()
        .blocks
        .iter()
        .flat_map(|b| &b.sentences)
        .find(|s| s.id == sentence)
        .and_then(|s| s.words.get(word))
        .map(|w| w.index)
        .expect("le mot existe")
}

/// Attend que le lecteur lise ce mot.
fn wait_word(session: &Session, what: &str, at: (usize, usize)) {
    let index = index_of(session, at);
    wait_for(session, what, |s| {
        s.sentence == at.0 && s.word == Some(index)
    });
}

fn seek(session: &Session, cx: &mut VisualTestContext, sentence: usize, word: usize) {
    session.send(Request::Command(Command::SeekToWord { sentence, word }));
    wait_word(session, "le saut", (sentence, word));
    tick(cx);
}

#[gpui::test]
fn highlights_the_current_word(cx: &mut TestAppContext) {
    let (session, view, cx) = open(cx, "Un deux trois. Quatre cinq six sept. Huit neuf.");
    seek(&session, cx, 1, 2);
    // `Snapshot.word` est l'index global du mot : « six » est le 6e mot du
    // document (3 + 2), pas la position 2 dans sa phrase.
    assert_eq!(session.snapshot().word, Some(5));
    view.read_with(cx, |v, _| {
        assert_eq!(v.highlighted(), Some((1, 2)));
        assert_eq!(v.highlighted_sentence(), 1);
    });

    // Rien ne change : ni nouvelle mise en page à chaque image, ni
    // rafraîchissement à chaque tour de la boucle.
    let (passes, updates) = view.read_with(cx, |v, _| (v.layout_passes(), v.updates()));
    for _ in 0..8 {
        tick(cx);
    }
    view.read_with(cx, |v, _| {
        assert_eq!(v.layout_passes(), passes);
        assert_eq!(v.updates(), updates);
    });
    // Une autre largeur refait la mise en page, une fois.
    cx.simulate_resize(size(px(400.0), px(300.0)));
    draw(cx);
    draw(cx);
    view.read_with(cx, |v, _| assert_eq!(v.layout_passes(), passes + 1));
}

#[gpui::test]
fn scrolling_detaches_and_follow_reattaches(cx: &mut TestAppContext) {
    let (session, view, cx) = open(cx, &paragraphs(40));
    seek(&session, cx, 25, 1);
    let followed = view.read_with(cx, |v, _| {
        assert!(v.following());
        v.scroll_offset()
    });
    assert!(
        followed > 0.0,
        "la ligne lue loin dans le texte fait défiler"
    );

    cx.simulate_event(ScrollEvent {
        position: point(px(300.0), px(150.0)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(120.0))),
        ..Default::default()
    });
    draw(cx);
    view.read_with(cx, |v, _| {
        assert!(!v.following(), "un défilement à la main détache la vue");
        assert_eq!(v.scroll_offset(), followed - 120.0);
    });
    // La lecture qui avance ne ramène pas la vue tant qu'on est détaché.
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.scroll_offset(), followed - 120.0));

    view.update(cx, |v, cx| v.follow(cx));
    draw(cx);
    view.read_with(cx, |v, _| {
        assert!(v.following());
        assert_eq!(
            v.scroll_offset(),
            followed,
            "follow() recentre la ligne lue"
        );
    });
}

#[gpui::test]
fn line_down_sends_seek_to_neighbour(cx: &mut TestAppContext) {
    // Une seule longue phrase : plusieurs lignes affichées.
    let long = "Un deux trois quatre cinq six sept huit neuf dix onze douze \
                treize quatorze quinze seize dix-sept dix-huit dix-neuf vingt.";
    let (session, view, cx) = open(cx, &format!("{long}\n\nFin du texte."));

    // Sans mot lu, la ligne voisine part de la première ligne de la phrase.
    view.read_with(cx, |v, _| {
        assert_eq!(v.highlighted(), None);
        assert_eq!(v.highlighted_sentence(), 0);
        assert!(
            v.layout().lines.len() >= 3,
            "la phrase tient sur plusieurs lignes"
        );
    });
    let first = view.read_with(cx, |v, _| {
        let start = v.layout().lines[0].words[0].clone();
        v.layout().neighbour(start.sentence, start.word, 1).unwrap()
    });
    view.update(cx, |v, cx| v.line(1, cx));
    wait_word(&session, "la ligne suivante", first);
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.highlighted(), Some(first)));

    // D'un mot lu, à la ligne d'après.
    let second = view.read_with(cx, |v, _| {
        v.layout().neighbour(first.0, first.1, 1).unwrap()
    });
    assert_ne!(second, first);
    view.update(cx, |v, cx| v.line(1, cx));
    wait_word(&session, "encore une ligne", second);
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.highlighted(), Some(second)));

    // Et retour.
    view.update(cx, |v, cx| v.line(-1, cx));
    wait_word(&session, "la ligne d'avant", first);
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.highlighted(), Some(first)));
}

#[gpui::test]
fn click_seeks_to_word(cx: &mut TestAppContext) {
    let (session, view, cx) = open(cx, "Un deux trois. Quatre cinq six. Sept huit neuf.");
    let at = view
        .read_with(cx, |v, _| v.word_position(2, 1))
        .expect("« huit » est affiché");
    cx.simulate_click(at, Modifiers::none());
    wait_word(&session, "le clic", (2, 1));
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.highlighted(), Some((2, 1))));
    // Un clic hors du texte ne fait rien.
    cx.simulate_click(point(px(590.0), px(290.0)), Modifiers::none());
    tick(cx);
    view.read_with(cx, |v, _| assert_eq!(v.highlighted(), Some((2, 1))));
}

/// Le document `doc`, prolongé de `extra` paragraphes : ids de phrase et
/// index de mot continuent, comme quand le cœur publie la suite du découpage.
fn grown(doc: &DocView, extra: usize) -> DocView {
    let mut doc = doc.clone();
    let mut sentence = doc.sentence_count;
    let mut word = doc
        .blocks
        .iter()
        .flat_map(|b| &b.sentences)
        .flat_map(|s| &s.words)
        .map(|w| w.index + 1)
        .max()
        .unwrap_or(0);
    for i in 0..extra {
        let words = ["Suite", "numéro", &i.to_string(), "ajoutée."]
            .iter()
            .map(|t| {
                word += 1;
                WordView {
                    index: word - 1,
                    text: t.to_string(),
                    ..Default::default()
                }
            })
            .collect();
        doc.blocks.push(BlockView {
            id: doc.blocks.len(),
            sentences: vec![SentenceView {
                id: sentence,
                words,
            }],
            ..Default::default()
        });
        sentence += 1;
    }
    doc.sentence_count = sentence;
    doc
}

#[gpui::test]
fn growing_document_keeps_word_and_scroll(cx: &mut TestAppContext) {
    let (session, view, cx) = open(cx, &paragraphs(30));
    seek(&session, cx, 20, 1);
    let followed = view.read_with(cx, |v, _| v.scroll_offset());
    assert!(followed > 0.0);

    // En suivant la lecture : la suite du document ne déplace rien.
    let snapshot = session.snapshot();
    let doc = session.document();
    let bigger = Arc::new(grown(&doc, 20));
    let next = Snapshot {
        revision: snapshot.revision + 1,
        ..snapshot.clone()
    };
    view.update(cx, |v, cx| v.receive(next.clone(), Arc::clone(&bigger), cx));
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(
            v.layout().lines.len(),
            50,
            "les blocs ajoutés sont mis en page"
        );
        assert_eq!(v.highlighted(), Some((20, 1)));
        assert_eq!(v.scroll_offset(), followed);
        assert!(v.following());
    });

    // Détaché, ailleurs dans le texte : la position choisie tient aussi.
    cx.simulate_event(ScrollEvent {
        position: point(px(300.0), px(150.0)),
        delta: ScrollDelta::Pixels(point(px(0.0), px(200.0))),
        ..Default::default()
    });
    draw(cx);
    let detached = view.read_with(cx, |v, _| v.scroll_offset());
    assert_eq!(detached, followed - 200.0);
    let biggest = Arc::new(grown(&bigger, 20));
    let after = Snapshot {
        revision: next.revision + 1,
        ..snapshot
    };
    view.update(cx, |v, cx| v.receive(after, biggest, cx));
    draw(cx);
    view.read_with(cx, |v, _| {
        assert_eq!(v.layout().lines.len(), 70);
        assert_eq!(v.highlighted(), Some((20, 1)));
        assert_eq!(v.scroll_offset(), detached);
        assert!(!v.following());
    });
}

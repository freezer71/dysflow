//! Rendu de l'interface.
//!
//! Deux principes commandent ce module.
//!
//! * **On ne dessine que ce qui est visible.** Le texte est découpé autour du
//!   curseur, puis replié à la largeur utile ; le coût d'une trame dépend de la
//!   taille de la fenêtre, pas de celle du document.
//! * **La hiérarchie se montre, elle ne se prononce pas.** Un titre se
//!   reconnaît à sa graisse, sa couleur et son filet, jamais à un marqueur lu à
//!   voix haute.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::doc::{BlockKind, Emphasis};
use crate::app::{App, Panel, ReadingStyle};
use crate::app::config::FIELDS;
use crate::app::keys::{config_help, help};
use crate::app::startup;

mod config;
mod player;
mod reader;

use config::*;
use player::*;
use reader::*;

pub(super) const ACCENT: Color = Color::Rgb(242, 107, 39);
pub(super) const BRIGHT: Color = Color::Rgb(238, 238, 238);
pub(super) const NORMAL: Color = Color::Rgb(186, 186, 186);
pub(super) const DIM: Color = Color::Rgb(120, 120, 120);
pub(super) const FAINT: Color = Color::Rgb(82, 82, 82);
/// Ce qui n'est pas de la prose : un lien, un fragment de code. Une teinte
/// froide, choisie loin de l'accent pour ne jamais passer pour le curseur.
pub(super) const TECHNICAL: Color = Color::Rgb(126, 168, 196);
/// Fond de la règle de lecture : assez marqué pour retrouver sa ligne d'un
/// coup d'œil, assez sombre pour ne pas concurrencer le mot prononcé.
pub(super) const RULER: Color = Color::Rgb(38, 38, 44);

/// Blocs conservés de part et d'autre du curseur.
///
/// Assez pour remplir n'importe quel écran et permettre un défilement fluide,
/// sans jamais parcourir le document entier.
pub(super) const WINDOW_BACK: usize = 40;
pub(super) const WINDOW_FORWARD: usize = 80;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if app.show_config {
        render_config(frame, app, area);
        if app.show_help {
            render_help(frame, area, config_help(), crate::tr!("Settings", "Réglages"));
        }
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // en-tête
            Constraint::Min(6),    // texte + panneau
            // Deux lignes de contenu plus les bordures : à 3, la seconde ligne
            // — moteur, avance de synthèse — était silencieusement rognée.
            Constraint::Length(4), // barre de lecture
            Constraint::Length(1), // statut
        ])
        .split(area);

    render_header(frame, app, chunks[0]);

    let body = if app.panel == Panel::None {
        chunks[1]
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(30), Constraint::Length(38)])
            .split(chunks[1]);
        render_panel(frame, app, split[1]);
        split[0]
    };

    render_reader(frame, app, body);
    render_player(frame, app, chunks[2]);
    render_status(frame, app, chunks[3]);

    if app.show_help {
        render_help(frame, area, help(), crate::tr!("Shortcuts", "Raccourcis"));
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let doc = app.doc();
    let section = doc
        .section_of_sentence(app.current_sentence())
        .map(|s| s.title.as_str())
        .unwrap_or("");

    let mut spans = vec![
        Span::styled(
            format!(" {} ", truncate(&doc.title, 40)),
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(FAINT)),
        Span::styled(truncate(&app.source_label(), 24), Style::default().fg(DIM)),
    ];
    if !section.is_empty() && section != doc.title {
        spans.push(Span::styled("  ▸ ", Style::default().fg(FAINT)));
        spans.push(Span::styled(truncate(section, 40), Style::default().fg(ACCENT)));
    }
    // Un flux encore ouvert doit se voir : le document n'est pas complet.
    if !app.input_complete() {
        spans.push(Span::styled(crate::tr!("  ⟳ stream", "  ⟳ flux"), Style::default().fg(DIM)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.to_string()).collect()).collect()
    }

    #[test]
    fn the_settings_are_one_page_of_six_lines() {
        let app = crate::app::tests::app_reading("Une phrase.\n");
        let page = texts(&config_lines(&app, 80));
        for field in FIELDS {
            assert!(page.iter().any(|l| l.contains(field.label())), "{field:?} manque : {page:?}");
        }
        for gone in ["Modèles", "Moteur", "Hauteur", "Blocs de code", "Langue"] {
            assert!(!page.iter().any(|l| l.contains(gone)), "{gone} est encore là : {page:?}");
        }
        let footer = texts(&config_footer(&app)).join(" ");
        assert!(footer.contains("enregistrée aussitôt"), "{footer}");
        assert!(!footer.contains(" w "), "{footer}");
    }

    /// Tout le texte d'une trame dessinée sur un terminal factice.
    fn screen_text(app: &App) -> String {
        let backend = ratatui::backend::TestBackend::new(110, 34);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let width = buffer.area.width as usize;
        let cells: Vec<&str> = buffer.content.iter().map(|c| c.symbol()).collect();
        cells.chunks(width).map(|row| row.concat()).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn in_english_the_screens_hold_no_french() {
        use crate::i18n::{with_ui_lang, UiLang};
        with_ui_lang(UiLang::En, || {
            let mut app = crate::app::tests::app_reading("One sentence.\n");
            let mut screens = vec![screen_text(&app)];
            app.show_help = true;
            screens.push(screen_text(&app));
            app.show_help = false;
            app.panel = Panel::Contents;
            screens.push(screen_text(&app));
            app.panel = Panel::None;
            app.show_config = true;
            for row in 0..FIELDS.len() {
                app.screen.row = row;
                screens.push(screen_text(&app));
            }
            app.show_help = true;
            screens.push(screen_text(&app));
            let all = screens.join("\n");
            for french in ['é', 'è', 'à', 'ç', 'ê', 'ô', 'û', 'ù', 'î'] {
                assert!(!all.contains(french), "« {french} » à l'écran :\n{all}");
            }
            for word in ["Réglages", "Raccourcis", "Sommaire", "Vitesse", "aide"] {
                assert!(!all.contains(word), "« {word} » à l'écran :\n{all}");
            }
            assert!(all.contains("Settings"), "{all}");
        });
    }

    #[test]
    fn the_status_line_tells_the_voice_is_installing() {
        let mut app = crate::app::tests::app_reading("Une phrase.\n");
        app.notices.clear();
        let package = crate::models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap();
        app.screen.download = Some(crate::models::Download::fake(package, false));
        let line = text_of(&status_spans(&app));
        assert!(line.contains("Installation de la voix"), "{line:?}");
        let footer = texts(&config_footer(&app)).join(" ");
        assert!(footer.contains("Installation de la voix"), "{footer}");
    }

    #[test]
    fn the_reminder_names_the_contents_and_no_bookmark() {
        assert!(REMINDER.get().contains("t sommaire"), "{REMINDER:?}");
        assert!(!REMINDER.get().contains("signet"), "{REMINDER:?}");
    }

    #[test]
    fn hints_fold_without_losing_a_word() {
        let text = "10 voix intégrées, au choix avec les flèches, t pour écouter";
        let lines = fold(text, 24);
        assert!(lines.iter().all(|l| l.chars().count() <= 24), "{lines:?}");
        assert_eq!(lines.join(" "), text);
        // Une largeur nulle rend le texte intact plutôt qu'une boucle infinie.
        assert_eq!(fold(text, 0), vec![text.to_string()]);
        // Un mot plus long que la ligne déborde au lieu d'être tronqué.
        assert_eq!(fold("anticonstitutionnellement", 5), vec!["anticonstitutionnellement"]);
    }

    #[test]
    fn clock_formats_hours_only_when_needed() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(65.0), "1:05");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(clock(-4.0), "0:00");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("court", 10), "court");
        let cut = truncate("élément très très long", 8);
        assert!(cut.chars().count() <= 8, "{cut:?}");
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn a_line_of_the_file_stays_one_line_on_screen() {
        // Deux lignes au fichier, deux lignes à l'écran : rien n'est recollé.
        let (lines, _) = layout(&["alpha beta", "gamma delta"], 40, 0);
        assert_eq!(text_of_lines(&lines), vec!["alpha beta", "gamma delta"]);
    }

    #[test]
    fn a_line_breaks_at_the_given_width() {
        let (lines, _) = layout(&["alpha beta gamma delta"], 12, 0);
        assert!(lines.len() >= 2, "aucun repli n'a eu lieu");
        for line in &lines {
            let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(width <= 12, "ligne trop large : {width}");
        }
    }

    #[test]
    fn one_line_too_wide_recomposes_the_whole_block() {
        // La composition du fichier vaut pour le bloc entier ou pour rien :
        // une ligne repliée au milieu de lignes intactes se lit en escalier.
        let (lines, _) = layout(&["alpha beta", "gamma delta epsilon zeta"], 14, 0);
        assert_eq!(
            text_of_lines(&lines),
            vec!["alpha beta", "gamma delta", "epsilon zeta"]
        );
    }

    #[test]
    fn layout_indents_every_line() {
        let source: Vec<String> = vec![(0..8).map(|i| format!("mot{i}")).collect::<Vec<_>>().join(" ")];
        let (lines, _) = layout(&source, 10, 4);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.spans[0].content.starts_with("    "), "{:?}", line.spans[0]);
        }
    }

    #[test]
    fn layout_keeps_overlong_words_whole() {
        let (lines, _) = layout(&["anticonstitutionnellement"], 8, 0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "anticonstitutionnellement");
    }

    #[test]
    fn the_spaces_of_a_line_are_kept_as_written() {
        let (lines, _) = layout(&["Nom      Rôle"], 40, 0);
        assert_eq!(text_of_lines(&lines), vec!["Nom      Rôle"]);
    }

    // ------------------------------------------------ confort de lecture

    fn mark() -> Mark {
        Mark { style: Style::default(), cursor: false }
    }

    /// Pose les mots de `source` sur ses propres lignes, sans style ni marqueur.
    fn layout(
        source: &[impl AsRef<str>],
        width: usize,
        indent: usize,
    ) -> (Vec<Line<'static>>, Option<usize>) {
        let source: Vec<String> = source.iter().map(|l| l.as_ref().to_string()).collect();
        let marks: Vec<Mark> = source
            .iter()
            .flat_map(|l| l.split_whitespace().map(|_| mark()).collect::<Vec<_>>())
            .collect();
        let prefix = Prefix { indent, marker: None, every_line: false };
        layout_tracked(&source, marks, width, &prefix, Style::default())
    }

    fn text_of_lines(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(|l| text_of(&l.spans)).collect()
    }

    #[test]
    fn reading_column_is_capped_and_centred() {
        let wide = reading_viewport(Rect::new(0, 0, 200, 40), 72);
        assert_eq!(wide.width, 72, "la colonne doit être bornée");
        // Centrée : autant d'espace de chaque côté.
        assert_eq!(wide.x, (200 - 72) / 2);
    }

    #[test]
    fn reading_column_keeps_margins_on_a_narrow_screen() {
        let narrow = reading_viewport(Rect::new(0, 0, 40, 10), 72);
        assert_eq!(narrow.width, 36, "deux colonnes de marge de chaque côté");
        assert_eq!(narrow.x, 2);
    }

    #[test]
    fn reading_column_zero_means_full_width() {
        let full = reading_viewport(Rect::new(0, 0, 200, 40), 0);
        assert_eq!(full.width, 196);
    }

    #[test]
    fn spacing_inserts_a_blank_line_between_lines_only() {
        let lines = vec![Line::from("a"), Line::from("b"), Line::from("c")];
        let spaced = space_out(lines, 1);
        assert_eq!(spaced.len(), 5, "a, vide, b, vide, c");
        assert_eq!(spaced[1].spans.len(), 0, "la deuxième ligne doit être vide");
        assert_eq!(spaced[4].spans[0].content.as_ref(), "c", "rien après la dernière");
    }

    #[test]
    fn spacing_of_zero_leaves_the_lines_untouched() {
        let lines = vec![Line::from("a"), Line::from("b")];
        assert_eq!(space_out(lines, 0).len(), 2);
    }

    #[test]
    fn ruled_line_spans_the_whole_width() {
        let line = ruled(Line::from("abc"), 10, Color::Rgb(1, 2, 3));
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 10, "la règle doit couvrir toute la colonne");
    }

    #[test]
    fn ruled_line_keeps_the_spoken_word_background() {
        let spoken = Span::styled("mot", Style::default().bg(ACCENT));
        let line = ruled(Line::from(vec![Span::raw("un "), spoken]), 12, Color::Rgb(1, 2, 3));
        let word = line.spans.iter().find(|s| s.content.as_ref() == "mot").unwrap();
        assert_eq!(word.style.bg, Some(ACCENT), "le mot prononcé garde son fond");
        let plain = line.spans.iter().find(|s| s.content.as_ref() == "un ").unwrap();
        assert_eq!(plain.style.bg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn layout_reports_the_line_holding_the_cursor() {
        let source = vec!["alpha beta gamma delta".to_string()];
        let mut marks: Vec<Mark> = (0..4).map(|_| mark()).collect();
        marks[3].cursor = true;
        let prefix = Prefix { indent: 0, marker: None, every_line: false };
        let (lines, cursor) = layout_tracked(&source, marks, 12, &prefix, Style::default());
        assert!(lines.len() >= 2);
        assert_eq!(cursor, Some(lines.len() - 1), "le curseur est sur la dernière ligne");
    }

    #[test]
    fn layout_reports_no_cursor_when_none_is_marked() {
        assert_eq!(layout(&["alpha beta"], 40, 0).1, None);
    }

    // ------------------------------------------------ mise en lignes du texte

    use crate::doc::builder::{parse_document, CodePolicy};

    const PROSE: &str =
        "Un paragraphe assez long pour occuper plusieurs lignes une fois replié.";


    fn block_of(doc: &crate::doc::Document, sentence: usize) -> usize {
        doc.sentence(sentence).map(|s| s.block_id).unwrap_or(0)
    }

    #[test]
    fn a_frozen_view_draws_around_its_own_block() {
        let doc = parse_document(
            "Un.\n\nDeux.\n\nTrois.\n\nQuatre.\n\nCinq.\n",
            CodePolicy::Announce,
        );
        let last = doc.sentences.len() - 1;
        // La lecture est à la fin, la vue est restée sur le premier bloc.
        let view = document_lines(ReaderFrame {
            current_sentence: last,
            ..ReaderFrame::new(&doc, 40)
        });
        let anchored = view.lines[view.anchor]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(anchored.contains("Un"), "ancre posée sur {anchored:?}");
    }

    #[test]
    fn the_line_being_spoken_is_ruled_across_the_column() {
        let doc = parse_document(PROSE, CodePolicy::Announce);
        let view = document_lines(ReaderFrame {
            current_word: Some(0),
            style: ReadingStyle { spacing: 0, ruler: true },
            ..ReaderFrame::new(&doc, 24)
        });
        let ruled: Vec<&Line> = view
            .lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| s.style.bg == Some(RULER)))
            .collect();
        assert_eq!(ruled.len(), 1, "une seule ligne doit porter la règle");
        let width: usize = ruled[0].spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 24, "la règle couvre toute la colonne");
    }

    #[test]
    fn no_ruler_when_it_is_turned_off() {
        let doc = parse_document(PROSE, CodePolicy::Announce);
        let view = document_lines(ReaderFrame {
            current_word: Some(0),
            ..ReaderFrame::new(&doc, 24)
        });
        assert!(view
            .lines
            .iter()
            .all(|l| l.spans.iter().all(|s| s.style.bg != Some(RULER))));
    }

    #[test]
    fn spacing_stretches_the_text_without_losing_a_word() {
        let doc = parse_document(PROSE, CodePolicy::Announce);
        let tight = document_lines(ReaderFrame::new(&doc, 24));
        let loose = document_lines(ReaderFrame {
            style: ReadingStyle { spacing: 1, ruler: false },
            ..ReaderFrame::new(&doc, 24)
        });
        assert!(loose.lines.len() > tight.lines.len(), "l'interlignage n'a rien changé");
        let text = |v: &ReaderView| {
            v.lines
                .iter()
                .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
                .collect::<String>()
        };
        assert_eq!(text(&tight), text(&loose), "un mot a disparu");
    }

    #[test]
    fn the_anchor_points_at_the_block_being_read() {
        let doc = parse_document("# Titre\n\nPremier.\n\nDeuxième.\n", CodePolicy::Announce);
        let last = doc.sentences.len() - 1;
        let view = document_lines(ReaderFrame {
            anchor_block: block_of(&doc, last),
            current_sentence: last,
            ..ReaderFrame::new(&doc, 40)
        });
        let anchored = view.lines[view.anchor]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(anchored.contains("Deuxième"), "ancre posée sur {anchored:?}");
    }

    #[test]
    fn announced_code_is_shown_instead_of_its_announcement() {
        let doc = parse_document("```rust\nfn a() {\n    b();\n}\n```", CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 40));
        let text: String = view
            .lines
            .iter()
            .map(|l| {
                l.spans.iter().map(|s| s.content.to_string()).collect::<String>() + "\n"
            })
            .collect();
        assert!(text.contains("    b();"), "l'indentation doit survivre : {text:?}");
        assert!(!text.contains("Bloc de code"), "l'annonce est dite, pas affichée");
    }

    #[test]
    fn overlong_code_lines_do_not_overflow_the_column() {
        let long = "x".repeat(80);
        let doc = parse_document(&format!("```\n{long}\n```"), CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 24));
        for line in &view.lines {
            let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(width <= 24, "ligne de {width} colonnes");
        }
        // Repliée, jamais tronquée : une ligne de code perdue est un sens perdu.
        let shown: String = view
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert_eq!(shown.matches('x').count(), 80, "du code a disparu");
    }

    #[test]
    fn a_folded_code_line_says_it_continues() {
        let long = "x".repeat(80);
        let lines = fixed_lines(&[long], 24, &BlockKind::Code { language: None }, false);
        assert!(lines.len() > 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "  │ ");
        assert_eq!(
            lines[1].spans[0].content.as_ref(),
            "  ╎ ",
            "la suite d'une ligne ne doit pas passer pour une ligne neuve"
        );
    }

    #[test]
    fn a_paragraph_keeps_the_lines_of_the_file() {
        // L'œil retrouve le fichier : mêmes lignes, même ordre, rien de recomposé.
        let doc = parse_document("Une première ligne\nune seconde ligne\n", CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 60));
        assert_eq!(shown_lines(&view), vec!["Une première ligne", "une seconde ligne"]);
    }

    #[test]
    fn an_aligned_column_keeps_its_spaces() {
        // L'alignement porte du sens : l'écraser revient à recomposer le texte.
        let doc = parse_document("Nom      Rôle\nalpha    serveur\n", CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 60));
        assert_eq!(shown_lines(&view), vec!["Nom      Rôle", "alpha    serveur"]);
    }

    #[test]
    fn a_block_too_wide_for_the_column_is_recomposed() {
        // Suivre les lignes d'un fichier plus large que la colonne laisserait
        // un mot orphelin sous chacune : mieux vaut alors le recomposer.
        let doc = parse_document(PROSE, CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 24));
        let shown = shown_lines(&view);
        assert!(shown.len() > 1, "aucun repli n'a eu lieu : {shown:?}");
        for line in &shown {
            assert!(line.chars().count() <= 24, "ligne trop large : {line:?}");
        }
        // Chaque ligne est remplie : le premier mot de la suivante n'y tenait
        // pas. Sans quoi le texte se lirait en escalier.
        for pair in shown.windows(2) {
            let next = pair[1].split(' ').next().unwrap_or_default();
            assert!(
                pair[0].chars().count() + 1 + next.chars().count() > 24,
                "un mot orphelin : {pair:?}"
            );
        }
        // Recomposé, jamais amputé : tous les mots, dans l'ordre.
        assert_eq!(shown.join(" "), PROSE);
    }

    #[test]
    fn a_word_still_carries_its_style_on_the_line_of_the_file() {
        // La composition vient du fichier, le style vient des mots : le
        // surlignage ne doit pas décrocher pour autant.
        let doc = parse_document("Une première ligne\nune seconde ligne\n", CodePolicy::Announce);
        let view = document_lines(ReaderFrame {
            current_word: Some(4),
            ..ReaderFrame::new(&doc, 60)
        });
        let spoken = view
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.style.bg == Some(ACCENT))
            .expect("le mot prononcé est affiché");
        assert_eq!(spoken.content.as_ref(), "seconde");
    }

    #[test]
    fn a_quote_carries_its_bar_on_every_line() {
        // Sans elle, la seconde ligne d'une citation se lit comme de la prose
        // décalée de deux colonnes.
        let doc = parse_document("> Une citation tenue\n> sur deux lignes.\n", CodePolicy::Announce);
        let view = document_lines(ReaderFrame::new(&doc, 60));
        let shown = shown_lines(&view);
        assert_eq!(shown.len(), 2, "{shown:?}");
        for line in &shown {
            assert!(line.starts_with("  │ "), "citation sans filet : {line:?}");
        }
    }

    #[test]
    fn a_list_item_hangs_under_its_own_text() {
        // La suite d'un item s'aligne sous son texte, pas sous sa puce : la
        // liste garde alors sa colonne.
        let doc = parse_document(
            "- Un item de liste assez long pour déborder de la colonne.\n",
            CodePolicy::Announce,
        );
        let view = document_lines(ReaderFrame::new(&doc, 30));
        let shown = shown_lines(&view);
        assert!(shown.len() > 1, "aucun repli : {shown:?}");
        assert!(shown[0].starts_with("  • "), "{:?}", shown[0]);
        assert!(
            shown[1].starts_with("    ") && !shown[1].starts_with("     "),
            "la suite ne pend pas sous le texte : {:?}",
            shown[1]
        );
    }

    /// Le style du premier span dont le texte contient `word`.
    fn style_of(view: &ReaderView, word: &str) -> Style {
        view.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains(word))
            .unwrap_or_else(|| panic!("le mot « {word} » est absent de l'écran"))
            .style
    }

    #[test]
    fn the_emphasis_of_the_file_is_shown() {
        // « Utiliser tout le formatage disponible sur la vue » : le gras du
        // fichier est du gras à l'écran, l'italique de l'italique.
        let doc = parse_document(
            "Un mot **en gras**, un autre en *italique*, un [lien](https://exemple.fr), du `code` et du ~~barré~~.",
            CodePolicy::Announce,
        );
        let view = document_lines(ReaderFrame::new(&doc, 90));
        assert!(style_of(&view, "gras").add_modifier.contains(Modifier::BOLD));
        assert!(style_of(&view, "italique").add_modifier.contains(Modifier::ITALIC));
        assert!(style_of(&view, "lien").add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(style_of(&view, "code").fg, Some(TECHNICAL));
        assert!(style_of(&view, "barré").add_modifier.contains(Modifier::CROSSED_OUT));
        // La prose reste de la prose : la mise en valeur ne dit quelque chose
        // que si elle est rare.
        let plain = style_of(&view, "mot");
        assert!(!plain.add_modifier.contains(Modifier::BOLD), "{plain:?}");
        assert!(!plain.add_modifier.contains(Modifier::ITALIC), "{plain:?}");
    }

    #[test]
    fn the_spoken_word_keeps_the_only_background_even_in_bold() {
        let doc = parse_document("Un mot **en gras** ici.", CodePolicy::Announce);
        let view = document_lines(ReaderFrame {
            current_word: Some(3),
            ..ReaderFrame::new(&doc, 60)
        });
        let spoken = style_of(&view, "gras");
        assert_eq!(spoken.bg, Some(ACCENT), "le mot prononcé prime");
    }

    #[test]
    fn a_faded_block_keeps_its_emphasis_without_its_colour() {
        // Hors du bloc lu, le texte s'efface en entier : un lien coloré y
        // ferait un trou dans la page.
        let doc = parse_document(
            "Un [lien](https://exemple.fr) ici.\n\nUne autre phrase.\n",
            CodePolicy::Announce,
        );
        let view = document_lines(ReaderFrame {
            current_sentence: 1,
            ..ReaderFrame::new(&doc, 60)
        });
        let link = style_of(&view, "lien");
        assert!(link.add_modifier.contains(Modifier::UNDERLINED));
        assert_eq!(link.fg, Some(FAINT), "le bloc effacé le reste");
    }

    /// Le texte de chaque ligne non vide, tel qu'il est à l'écran.
    fn shown_lines(view: &ReaderView) -> Vec<String> {
        view.lines
            .iter()
            .map(|l| text_of(&l.spans))
            .filter(|t| !t.trim().is_empty())
            .collect()
    }

    #[test]
    fn search_results_stand_out_in_the_text() {
        use crate::app::search::Search;
        let doc = parse_document("Le service expose deux routes.", CodePolicy::Announce);
        let search = Search::new(&doc, "route", 0);
        let view = document_lines(ReaderFrame {
            search: Some(&search),
            ..ReaderFrame::new(&doc, 60)
        });
        let found = view
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("routes"))
            .expect("le mot est affiché");
        assert!(
            found.style.add_modifier.contains(Modifier::UNDERLINED),
            "un résultat doit se distinguer : {:?}",
            found.style
        );
        let other = view
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("service"))
            .expect("le mot est affiché");
        assert!(!other.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn the_spoken_word_keeps_its_mark_even_when_it_is_a_result() {
        use crate::app::search::Search;
        let doc = parse_document("Une route.", CodePolicy::Announce);
        let search = Search::new(&doc, "route", 0);
        let view = document_lines(ReaderFrame {
            current_word: Some(1),
            search: Some(&search),
            ..ReaderFrame::new(&doc, 60)
        });
        let spoken = view
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("route"))
            .expect("le mot est affiché");
        assert_eq!(spoken.style.bg, Some(ACCENT), "le mot prononcé prime");
    }

    fn text_of(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    #[test]
    fn the_status_line_shows_the_query_being_typed() {
        let mut app = crate::app::tests::app_reading("Une phrase.\n");
        app.search_open();
        app.search_push('r');
        app.search_push('o');
        let line = text_of(&status_spans(&app));
        assert!(line.contains("ro"), "la saisie doit se voir : {line:?}");
        assert!(line.contains("Échap"), "la sortie doit être dite : {line:?}");
    }

    #[test]
    fn the_status_line_keeps_saying_that_the_view_is_frozen() {
        let mut app = crate::app::tests::app_reading("Une phrase.\n\nUne autre.\n");
        app.scroll_view(-1);
        // Un autre message chasse la notification, pas l'état.
        app.notify("autre chose");
        let line = text_of(&status_spans(&app));
        assert!(line.contains("figée"), "état perdu : {line:?}");
    }

    #[test]
    fn a_table_holds_together_as_one_grid() {
        let doc = parse_document(
            "| Route | Méthode |\n|---|---|\n| /users | GET |\n| /a | DELETE |\n",
            CodePolicy::Announce,
        );
        let view = document_lines(ReaderFrame::new(&doc, 60));
        let shown: Vec<String> = view.lines.iter().map(|l| text_of(&l.spans)).collect();
        let first = shown
            .iter()
            .position(|l| l.contains("Route"))
            .expect("l'en-tête est affiché");
        // Un tableau porte déjà sa grille : un filet de gauche en ferait une
        // seconde par-dessus.
        assert!(shown[first].starts_with("    Route"), "{:?}", shown[first]);
        // Ses lignes se suivent : une ligne vide entre elles en ferait autant
        // de tableaux.
        for line in &shown[first..first + 4] {
            assert!(!line.trim().is_empty(), "tableau disloqué : {shown:?}");
        }
        assert!(shown[first + 1].contains('┼'), "l'en-tête porte son filet");
    }

    #[test]
    fn a_table_is_shown_in_columns_with_its_header_apart() {
        let doc = parse_document(
            "| Route | Méthode |\n|---|---|\n| /users | GET |\n",
            CodePolicy::Announce,
        );
        let view = document_lines(ReaderFrame::new(&doc, 60));
        let shown: Vec<String> = view
            .lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.to_string()).collect())
            .collect();
        assert!(
            shown.iter().any(|l| l.contains("Route  │ Méthode")),
            "colonnes non alignées : {shown:?}"
        );
        // La valeur reste écrite comme dans le document, même dite autrement.
        assert!(shown.iter().any(|l| l.contains("/users │ GET")));

        let header = view
            .lines
            .iter()
            .find(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.contains("Route"))
            })
            .expect("l'en-tête est affiché");
        assert!(
            header
                .spans
                .iter()
                .any(|s| s.style.add_modifier.contains(Modifier::BOLD)),
            "l'en-tête doit se distinguer de ses lignes"
        );
    }


    #[test]
    fn wide_characters_count_for_the_columns_they_take() {
        use unicode_width::UnicodeWidthStr;
        // Un idéogramme prend deux colonnes : compté pour une, la ligne
        // déborde et le mot prononcé peut sortir de l'écran.
        let (lines, _) = layout(&["漢字漢字 漢字漢字 漢字漢字"], 12, 0);
        assert!(lines.len() >= 3, "{lines:?}");
        for line in &lines {
            let width: usize = line.spans.iter().map(|s| s.content.width()).sum();
            assert!(width <= 12, "ligne trop large : {width}");
        }
    }
}

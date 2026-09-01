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
use crate::models;
use crate::tui::app::{App, Panel};
use crate::tui::config::{Page, Section};
use crate::tui::keymap::{CONFIG_HELP, HELP};
use crate::tui::startup;

const ACCENT: Color = Color::Rgb(242, 107, 39);
const BRIGHT: Color = Color::Rgb(238, 238, 238);
const NORMAL: Color = Color::Rgb(186, 186, 186);
const DIM: Color = Color::Rgb(120, 120, 120);
const FAINT: Color = Color::Rgb(82, 82, 82);
/// Ce qui n'est pas de la prose : un lien, un fragment de code. Une teinte
/// froide, choisie loin de l'accent pour ne jamais passer pour le curseur.
const TECHNICAL: Color = Color::Rgb(126, 168, 196);
/// Fond de la règle de lecture : assez marqué pour retrouver sa ligne d'un
/// coup d'œil, assez sombre pour ne pas concurrencer le mot prononcé.
const RULER: Color = Color::Rgb(38, 38, 44);

/// Blocs conservés de part et d'autre du curseur.
///
/// Assez pour remplir n'importe quel écran et permettre un défilement fluide,
/// sans jamais parcourir le document entier.
const WINDOW_BACK: usize = 40;
const WINDOW_FORWARD: usize = 80;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if app.show_config {
        render_config(frame, app, area);
        if app.show_help {
            render_help(frame, area, CONFIG_HELP, "Configuration");
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
        render_help(frame, area, HELP, "Raccourcis");
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
        Span::styled(truncate(&app.source, 24), Style::default().fg(DIM)),
    ];
    if !section.is_empty() && section != doc.title {
        spans.push(Span::styled("  ▸ ", Style::default().fg(FAINT)));
        spans.push(Span::styled(truncate(section, 40), Style::default().fg(ACCENT)));
    }
    // Un flux encore ouvert doit se voir : le document n'est pas complet.
    if !app.input_complete() {
        spans.push(Span::styled("  ⟳ flux", Style::default().fg(DIM)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Ce qu'un mot apporte à la ligne où le fichier l'a écrit.
///
/// Le texte, lui, vient du document : c'est sa composition qui est à l'écran,
/// et les mots ne font que la styler, dans l'ordre.
struct Mark {
    style: Style,
    /// Vrai pour le mot en train d'être prononcé.
    cursor: bool,
}

fn render_reader(frame: &mut Frame, app: &App, area: Rect) {
    // Une ligne trop large se relit mal : on perd le début de la suivante.
    let viewport = reading_viewport(area, app.config.reading.line_width as usize);
    let width = viewport.width as usize;
    if width < 8 || viewport.height == 0 {
        return;
    }

    let doc = app.doc();
    if doc.is_empty() {
        let message = if app.input_complete() {
            "Aucun contenu lisible dans l'entrée."
        } else {
            "En attente du flux…"
        };
        frame.render_widget(
            Paragraph::new(message).style(Style::default().fg(DIM)),
            viewport,
        );
        return;
    }

    let view = document_lines(ReaderFrame {
        anchor_block: app.anchor_block(),
        current_sentence: app.current_sentence(),
        current_word: app.current_word(),
        style: app.reading_style(),
        search: app.search(),
        ..ReaderFrame::new(doc, width)
    });

    // Le point d'ancrage reste au tiers supérieur : on voit ce qui vient.
    let scroll = view.anchor.saturating_sub(viewport.height as usize / 3);

    frame.render_widget(
        Paragraph::new(view.lines).scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        viewport,
    );
}

/// Indentation et marqueur visuel d'un bloc.
fn block_prefix(kind: &BlockKind) -> (usize, Option<(String, Style)>) {
    match kind {
        BlockKind::Heading { .. } => (0, None),
        BlockKind::Paragraph => (0, None),
        BlockKind::ListItem { depth, ordinal } => {
            let indent = 2 + *depth as usize * 2;
            let marker = match ordinal {
                Some(n) => format!("{n}."),
                None => "•".to_string(),
            };
            (indent, Some((marker, Style::default().fg(ACCENT))))
        }
        BlockKind::Quote => (2, Some(("│".to_string(), Style::default().fg(FAINT)))),
        BlockKind::Code { .. } => (2, Some(("⟨⟩".to_string(), Style::default().fg(FAINT)))),
        BlockKind::TableRow { .. } => (2, Some(("·".to_string(), Style::default().fg(FAINT)))),
    }
}

/// Ajoute au mot la mise en valeur qu'il porte dans le fichier.
///
/// Jamais de fond : le mot prononcé reste le seul de l'écran à en porter un.
/// Et jamais de couleur hors du bloc lu, qui doit s'effacer en entier — la
/// graisse et le soulignement, eux, restent lisibles sans faire de trou dans
/// la page.
fn emphasised(style: Style, emphasis: &Emphasis, in_current_block: bool) -> Style {
    let mut style = style;
    if emphasis.strong {
        style = style.add_modifier(Modifier::BOLD);
    }
    if emphasis.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if emphasis.struck {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if emphasis.link {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if (emphasis.link || emphasis.code) && in_current_block {
        style = style.fg(TECHNICAL);
    }
    style
}

fn word_style(
    kind: &BlockKind,
    emphasis: &Emphasis,
    in_current_block: bool,
    in_current_sentence: bool,
    is_current_word: bool,
) -> Style {
    if is_current_word {
        // Le mot prononcé : le seul élément à fond plein de tout l'écran.
        return Style::default()
            .bg(ACCENT)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
    }

    let base = match kind {
        BlockKind::Heading { level: 1 } => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        BlockKind::Heading { level: 2 } => Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        BlockKind::Heading { .. } => Style::default().fg(NORMAL).add_modifier(Modifier::BOLD),
        BlockKind::Quote => Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        BlockKind::Code { .. } => Style::default().fg(DIM).add_modifier(Modifier::DIM),
        BlockKind::TableRow { header: true } => {
            Style::default().fg(NORMAL).add_modifier(Modifier::BOLD)
        }
        BlockKind::TableRow { header: false } => Style::default().fg(DIM),
        _ => Style::default().fg(NORMAL),
    };

    // La phrase lue s'éclaircit, elle ne s'épaissit pas : la graisse dit
    // désormais le gras du fichier, et deux signaux pour deux choses valent
    // mieux qu'un seul pour les deux. La règle de lecture et le mot prononcé
    // la désignent par ailleurs.
    let base = if in_current_sentence {
        base.fg(BRIGHT)
    } else if in_current_block {
        base
    } else {
        // Hors du bloc courant, le texte s'efface sans disparaître.
        base.fg(FAINT).remove_modifier(Modifier::BOLD)
    };
    emphasised(base, emphasis, in_current_block)
}

/// Colonne de lecture : bornée en largeur et centrée dans `area`.
///
/// Une ligne de cent quatre-vingts caractères se relit mal — on perd le début
/// de la suivante. `max` à zéro rend la largeur utile entière.
fn reading_viewport(area: Rect, max: usize) -> Rect {
    const MARGIN: u16 = 2;
    let usable = area.width.saturating_sub(MARGIN * 2);
    let width = if max == 0 { usable } else { usable.min(max as u16) };
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

/// Insère `spacing` lignes vides entre les lignes, jamais après la dernière.
fn space_out(lines: Vec<Line<'static>>, spacing: usize) -> Vec<Line<'static>> {
    if spacing == 0 {
        return lines;
    }
    let last = lines.len().saturating_sub(1);
    let mut out = Vec::with_capacity(lines.len() * (spacing + 1));
    for (i, line) in lines.into_iter().enumerate() {
        out.push(line);
        if i < last {
            out.extend(std::iter::repeat_with(Line::default).take(spacing));
        }
    }
    out
}

/// Étend une ligne sur toute la largeur et lui pose un fond.
///
/// Les spans qui portent déjà un fond — le mot prononcé — gardent le leur.
fn ruled(line: Line<'static>, width: usize, background: Color) -> Line<'static> {
    let mut spans = line.spans;
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    for span in spans.iter_mut() {
        // Le mot prononcé porte déjà un fond : la règle passe dessous.
        if span.style.bg.is_none() {
            span.style = span.style.bg(background);
        }
    }
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(background),
        ));
    }
    Line::from(spans)
}

/// Découpe une ligne en mots, chacun précédé de son écart.
///
/// L'écart est compté plutôt qu'écrasé : c'est lui qui aligne une colonne.
fn cells(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut gap = 0usize;
    let mut word = String::new();
    for c in source.chars() {
        if c.is_whitespace() {
            if !word.is_empty() {
                out.push((gap, std::mem::take(&mut word)));
                gap = 0;
            }
            gap += 1;
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push((gap, word));
    }
    out
}

/// Pose les mots sur les lignes du fichier et signale celle qui est prononcée.
///
/// La composition vient du document — mêmes lignes, mêmes écarts ; le style,
/// lui, vient des mots, consommés dans l'ordre.
///
/// Tant qu'elle tient dans la colonne. Au-delà, la suivre coûterait plus
/// qu'elle ne rapporte : un fichier plié à quatre-vingts colonnes, lu dans une
/// colonne plus étroite, laisserait un mot orphelin sous chacune de ses
/// lignes. Le bloc est alors recomposé d'un seul tenant — ce qui est encore la
/// façon la plus fidèle de le montrer quand l'écran est trop étroit pour lui.
///
/// La ligne du mot prononcé est celle que la règle de lecture souligne : sans
/// elle, retrouver où l'on en est demande de balayer l'écran à chaque retour
/// d'attention.
fn layout_tracked(
    source: &[String],
    marks: Vec<Mark>,
    width: usize,
    indent: usize,
    base: Style,
    prefix: Option<(String, Style)>,
) -> (Vec<Line<'static>>, Option<usize>) {
    if width == 0 {
        return (Vec::new(), None);
    }
    // Le marqueur du bloc occupe le début de la première ligne : il compte
    // dans ce qu'elle peut tenir.
    let head = prefix.as_ref().map_or(0, |(text, _)| text.chars().count() + 1);
    let recomposed: Vec<String>;
    let source: &[String] = if source
        .iter()
        .enumerate()
        .all(|(n, line)| indent + usize::from(n == 0) * head + line.chars().count() <= width)
    {
        source
    } else {
        recomposed = vec![source.join(" ")];
        &recomposed
    };

    let mut words = marks.into_iter();
    let mut prefix = prefix;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_line = None;

    for (n, source_line) in source.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut used = indent;
        if indent > 0 {
            spans.push(Span::raw(" ".repeat(indent)));
        }
        // Le marqueur du bloc — puce, filet de citation — ouvre sa première
        // ligne et se comporte ensuite comme un mot de plus.
        if n == 0 {
            if let Some((text, style)) = prefix.take() {
                used += text.chars().count();
                spans.push(Span::styled(text, style));
            }
        }
        let mut placed = used > indent;

        for (written, text) in cells(source_line) {
            // L'écart du fichier ouvre la ligne — c'est son indentation — et
            // sépare ensuite deux mots sans jamais les coller.
            let mut gap = if placed { written.max(1) } else { written };
            let len = text.chars().count();
            if placed && used + gap + len > width {
                lines.push(Line::from(std::mem::take(&mut spans)));
                used = indent;
                if indent > 0 {
                    spans.push(Span::raw(" ".repeat(indent)));
                }
                gap = 0;
            }
            if gap > 0 {
                spans.push(Span::raw(" ".repeat(gap)));
                used += gap;
            }
            // Un mot plus long que la colonne est laissé intact : le tronquer
            // nuirait plus à la lecture que de déborder.
            let (style, cursor) = match words.next() {
                Some(mark) => (mark.style, mark.cursor),
                None => (base, false),
            };
            if cursor {
                cursor_line = Some(lines.len());
            }
            spans.push(Span::styled(text, style));
            used += len;
            placed = true;
        }

        // Une ligne vide du fichier reste vide à l'écran : elle en compose la
        // respiration.
        lines.push(if placed { Line::from(spans) } else { Line::default() });
    }
    (lines, cursor_line)
}

/// Réglages d'affichage du texte, tels que la rubrique « Lecture » les fixe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingStyle {
    /// Lignes vides insérées entre deux lignes de texte.
    pub spacing: usize,
    /// Souligne d'un fond la ligne où se trouve le mot prononcé.
    pub ruler: bool,
}

/// Lignes à afficher, et index de celle qui ouvre le bloc en cours de lecture.
struct ReaderView {
    lines: Vec<Line<'static>>,
    anchor: usize,
}

/// Style d'un bloc affiché tel qu'il est écrit : texte, puis filet.
fn fixed_style(kind: &BlockKind, current: bool) -> (Style, Style) {
    let text = match kind {
        // Un en-tête de tableau nomme les colonnes de tout ce qui suit : il
        // doit se distinguer de ses lignes sans les écraser.
        BlockKind::TableRow { header: true } => Style::default()
            .fg(if current { BRIGHT } else { NORMAL })
            .add_modifier(Modifier::BOLD),
        _ if current => Style::default().fg(NORMAL),
        _ => Style::default().fg(FAINT),
    };
    let gutter = Style::default().fg(if current { ACCENT } else { FAINT });
    (text, gutter)
}

/// Filets d'un bloc montré tel qu'il est écrit : ouverture, puis continuation.
///
/// Le tableau fait exception : il porte déjà sa grille, et un filet de plus à
/// gauche en dessinerait une seconde par-dessus la sienne. Les deux marques
/// gardent la même largeur, sans quoi les colonnes se décaleraient au premier
/// repli.
fn gutters(kind: &BlockKind) -> (&'static str, &'static str) {
    match kind {
        BlockKind::TableRow { .. } => ("    ", "  ╎ "),
        _ => ("  │ ", "  ╎ "),
    }
}

/// Met un source en lignes, replié plutôt que tronqué.
///
/// Un filet en tête distingue le code de la prose sans qu'aucun marqueur ne
/// soit prononcé, et la continuation d'une ligne trop longue reste alignée
/// dessous : rien de ce qui est écrit ne disparaît.
fn fixed_lines(
    source: &[String],
    width: usize,
    kind: &BlockKind,
    current: bool,
) -> Vec<Line<'static>> {
    let (style, gutter) = fixed_style(kind, current);
    let (opening, continuation) = gutters(kind);
    let room = width.saturating_sub(opening.chars().count()).max(1);

    let mut lines = Vec::new();
    for source_line in source {
        let mut rest: Vec<char> = source_line.chars().collect();
        if rest.is_empty() {
            lines.push(Line::from(Span::styled(opening.trim_end().to_string(), gutter)));
            continue;
        }
        let mut first = true;
        while !rest.is_empty() {
            let take = rest.len().min(room);
            let chunk: String = rest.drain(..take).collect();
            lines.push(Line::from(vec![
                Span::styled(if first { opening } else { continuation }.to_string(), gutter),
                Span::styled(chunk, style),
            ]));
            first = false;
        }
    }
    lines
}

/// Tout ce dont la mise en lignes a besoin.
///
/// Regroupés plutôt qu'alignés en paramètres : `anchor_block` et
/// `current_sentence` sont deux entiers voisins que rien n'empêcherait
/// d'intervertir.
struct ReaderFrame<'a> {
    doc: &'a crate::doc::Document,
    /// Bloc autour duquel dessiner.
    anchor_block: usize,
    current_sentence: usize,
    current_word: Option<usize>,
    width: usize,
    style: ReadingStyle,
    /// Recherche en cours, dont les occurrences se signalent dans le texte.
    search: Option<&'a crate::tui::search::Search>,
}

impl<'a> ReaderFrame<'a> {
    fn new(doc: &'a crate::doc::Document, width: usize) -> Self {
        Self {
            doc,
            anchor_block: 0,
            current_sentence: 0,
            current_word: None,
            width,
            style: ReadingStyle { spacing: 0, ruler: false },
            search: None,
        }
    }
}

/// Met le document en lignes autour du curseur.
///
/// Séparée du rendu pour être vérifiable : c'est ici que se décident le repli,
/// l'interlignage et la règle de lecture.
fn document_lines(frame: ReaderFrame<'_>) -> ReaderView {
    let ReaderFrame {
        doc,
        anchor_block,
        current_sentence,
        current_word,
        width,
        style,
        search,
    } = frame;
    let mut lines: Vec<Line> = Vec::new();
    let mut anchor = 0usize;
    if doc.blocks.is_empty() || width == 0 {
        return ReaderView { lines, anchor };
    }

    // La fenêtre est dessinée autour de l'ancre — le bloc lu quand la vue
    // suit la voix, celui qu'on relit quand elle est figée. Le surlignage,
    // lui, reste attaché à ce qui est prononcé.
    let current_block = doc
        .sentence(current_sentence)
        .map(|s| s.block_id)
        .unwrap_or(0);
    let anchor_block = anchor_block.min(doc.blocks.len() - 1);
    let first = anchor_block.saturating_sub(WINDOW_BACK);
    let last = (anchor_block + WINDOW_FORWARD).min(doc.blocks.len() - 1);

    let window = &doc.blocks[first..=last];
    for (n, block) in window.iter().enumerate() {
        if block.id == anchor_block {
            anchor = lines.len();
        }
        let is_current_block = block.id == current_block;
        // Les lignes d'un même tableau se tiennent : une ligne vide entre
        // elles en ferait autant de tableaux.
        let glued = matches!(block.kind, BlockKind::TableRow { .. })
            && window
                .get(n + 1)
                .is_some_and(|next| matches!(next.kind, BlockKind::TableRow { .. }));

        // Le code et les tableaux ne se disent pas comme ils s'écrivent : ce
        // qu'ils montrent n'a pas de mots à suivre, et se montre tel quel
        // pendant que la voix n'en dit que le nom.
        if matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. }) {
            let shown = fixed_lines(&block.display_lines, width, &block.kind, is_current_block);
            // L'interlignage aère la prose ; dans un tableau, il séparerait
            // l'en-tête du filet qui le souligne.
            lines.extend(if matches!(block.kind, BlockKind::TableRow { .. }) {
                shown
            } else {
                space_out(shown, style.spacing)
            });
            if !glued {
                lines.push(Line::from(""));
            }
            continue;
        }

        let (indent, prefix) = block_prefix(&block.kind);
        let mut marks: Vec<Mark> = Vec::new();

        for id in block.sentences.0..block.sentences.1 {
            let Some(sentence) = doc.sentence(id) else { continue };
            let is_current_sentence = id == current_sentence;
            for (i, word) in sentence.words.iter().enumerate() {
                let is_current_word = is_current_sentence && current_word == Some(i);
                let mut style = word_style(
                    &block.kind,
                    &word.emphasis,
                    is_current_block,
                    is_current_sentence,
                    is_current_word,
                );
                // Un résultat de recherche se souligne — le fond plein reste
                // réservé au mot prononcé, qui ne doit jamais se confondre.
                if !is_current_word
                    && search.is_some_and(|s| s.matches_word(word))
                {
                    style = style.fg(BRIGHT).add_modifier(Modifier::UNDERLINED);
                }
                marks.push(Mark { style, cursor: is_current_word });
            }
        }

        // La composition vient du fichier : mêmes lignes, mêmes écarts. Les
        // mots n'y apportent que leur style — et le bloc son texte, si le
        // document n'a pas su en garder les lignes.
        let source = if block.display_lines.is_empty() {
            std::slice::from_ref(&block.text)
        } else {
            &block.display_lines
        };
        let (mut wrapped, spoken_line) = layout_tracked(
            source,
            marks,
            width,
            indent,
            word_style(&block.kind, &Emphasis::default(), is_current_block, false, false),
            prefix,
        );
        // La règle passe sous la ligne entière : c'est elle qu'on retrouve en
        // relevant les yeux, pas le mot seul.
        if style.ruler {
            if let Some(index) = spoken_line {
                let line = std::mem::take(&mut wrapped[index]);
                wrapped[index] = ruled(line, width, RULER);
            }
        }
        lines.extend(space_out(wrapped, style.spacing));

        // Un titre est souligné d'un filet : la hiérarchie se lit d'un coup
        // d'œil sans qu'aucun marqueur ne soit prononcé.
        if let BlockKind::Heading { level } = block.kind {
            if level <= 2 {
                let rule_width = width.min(if level == 1 { 48 } else { 28 });
                lines.push(Line::from(Span::styled(
                    "─".repeat(rule_width),
                    Style::default().fg(if level == 1 { ACCENT } else { FAINT }),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    ReaderView { lines, anchor }
}

fn render_player(frame: &mut Frame, app: &App, area: Rect) {
    let doc = app.doc();
    let current = app.current_sentence();
    let total = doc.sentences.len().max(1);
    let stats = app.synth.stats();

    let icon = if app.player.is_playing() { "⏸" } else { "▶" };
    let ratio = (current as f64 / total as f64).clamp(0.0, 1.0);

    let inner_width = area.width.saturating_sub(4) as usize;
    let bar_width = inner_width.saturating_sub(46).max(10);
    let filled = (bar_width as f64 * ratio).round() as usize;

    let times = match (app.estimated_elapsed(), app.estimated_total()) {
        (Some(elapsed), Some(total_secs)) => {
            format!("{} / ~{}", clock(elapsed), clock(total_secs))
        }
        // Aucune mesure encore : afficher un temps serait l'inventer.
        _ => "--:-- / --:--".to_string(),
    };

    // Avant le premier son, la barre de progression n'a rien à montrer : elle
    // cède la place à l'étape en cours. Trois secondes de chargement sans
    // retour donnaient un lecteur qui paraissait figé.
    let first = match app.startup_phase() {
        Some(phase) => {
            let elapsed = app.startup_elapsed();
            Line::from(vec![
                Span::styled(
                    format!(" {}  ", startup::spinner(elapsed)),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    phase.label(&app.config.engine.family),
                    Style::default().fg(NORMAL),
                ),
                Span::styled(
                    format!("   {:.1} s", elapsed.as_secs_f32()),
                    Style::default().fg(DIM),
                ),
            ])
        }
        None => Line::from(vec![
        Span::styled(
            format!(" {icon} "),
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("█".repeat(filled), Style::default().fg(ACCENT)),
        Span::styled(
            "─".repeat(bar_width.saturating_sub(filled)),
            Style::default().fg(FAINT),
        ),
        Span::raw("  "),
        Span::styled(times, Style::default().fg(NORMAL)),
        Span::raw("  "),
        Span::styled(
            format!("{}/{}", current + 1, total),
            Style::default().fg(DIM),
        ),
        ]),
    };

    let mut second = vec![
        Span::styled(
            format!(" {:.2}× ", app.config.voice.speed),
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if app.player.is_muted() {
                "  🔇 coupé ".to_string()
            } else {
                format!("  🔊 {:>3.0} % ", app.player.volume() * 100.0)
            },
            Style::default().fg(NORMAL),
        ),
        Span::styled(
            format!("  {} ", app.engine_name()),
            Style::default().fg(DIM),
        ),
        // L'avance de synthèse est visible : on sait si le moteur suit.
        Span::styled(
            format!(
                " ⟐ {} prêtes · {} en cours · {} sessions",
                stats.ready, stats.in_flight, stats.workers
            ),
            Style::default().fg(FAINT),
        ),
    ];
    if stats.failed > 0 {
        second.push(Span::styled(
            format!("  ⚠ {} échec(s)", stats.failed),
            Style::default().fg(ACCENT),
        ));
    }
    if app.bookmarked_here() {
        second.push(Span::styled("  ⚑ signet", Style::default().fg(ACCENT)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FAINT));

    frame.render_widget(
        Paragraph::new(vec![first, Line::from(second)]).block(block),
        area,
    );
}

/// Contenu de la ligne d'état.
///
/// Séparé du rendu : c'est la seule ligne qui parle à l'utilisateur en
/// permanence, et ce qu'elle dit se vérifie.
fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    // Une saisie en cours prend toute la ligne : ce qu'on tape doit se voir.
    if let Some(query) = app.search_input() {
        spans.push(Span::styled(
            " / ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {query}▏"),
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "  Entrée pour chercher · Échap pour annuler ",
            Style::default().fg(FAINT),
        ));
        return spans;
    }

    // Un repli sur la voix du système est un état dégradé : il doit se voir en
    // permanence, et dire comment en sortir. C'est la seule façon de rendre
    // l'écran de configuration découvrable sans avoir lu de documentation.
    let on_fallback = !models::available().is_empty() && models::first_installed().is_none();

    // Un modèle qui ne sait que cloner et n'a pas de référence ne produira
    // jamais un son : le dire ici, en permanence, plutôt qu'une fois par phrase
    // dans un journal que personne n'ouvre.
    if app.reference_missing() {
        spans.push(Span::styled(
            " ⚠ aucune voix de référence ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" ce modèle clone : ", Style::default().fg(NORMAL)));
        spans.push(Span::styled(
            "s",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " puis « Voix » pour désigner un WAV ",
            Style::default().fg(NORMAL),
        ));
        return spans;
    }

    // La vue figée est un état, pas un événement : il survit au message qui
    // l'a annoncé, sans quoi on cherche pourquoi le texte ne défile plus.
    if !app.view.is_following() {
        spans.push(Span::styled(
            " ⏸ vue figée ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "f revient à la lecture · Pg↑ Pg↓ défilent ",
            Style::default().fg(NORMAL),
        ));
    } else if !app.status().is_empty() {
        spans.push(Span::styled(
            format!(" {} ", app.status()),
            Style::default().fg(ACCENT),
        ));
    } else if on_fallback {
        spans.push(Span::styled(
            " ⚠ voix système ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" appuyez sur ", Style::default().fg(NORMAL)));
        spans.push(Span::styled(
            "s",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " pour installer un vrai modèle ",
            Style::default().fg(NORMAL),
        ));
    } else if let Some(notice) = app.notices.last() {
        spans.push(Span::styled(format!(" {notice} "), Style::default().fg(DIM)));
    } else {
        // Le rappel permanent nomme la configuration en premier.
        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(
            "s",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" réglages · ", Style::default().fg(FAINT)));
        spans.push(Span::styled(
            "?",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " aide · espace lecture · / chercher · n/p titre · b signet ",
            Style::default().fg(FAINT),
        ));
    }

    // Où en est la recherche, tant qu'elle a des résultats à parcourir.
    if let Some(search) = app.search() {
        if !search.is_empty() {
            spans.push(Span::styled(
                format!("· « {} » {}/{} ", search.query, search.position(), search.count()),
                Style::default().fg(NORMAL),
            ));
        }
    }

    // Indicateur de ponctuation : quelle marque le lecteur est en train de dire.
    if let Some(punctuation) = current_punctuation(app) {
        spans.push(Span::styled(
            format!("· {} ", punctuation),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    }
    if app.skipped_count() > 0 {
        spans.push(Span::styled(
            format!("· {} phrase(s) sautée(s) ", app.skipped_count()),
            Style::default().fg(DIM),
        ));
    }
    if app.is_finished() {
        spans.push(Span::styled("· lecture terminée ", Style::default().fg(DIM)));
    }
    spans
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(status_spans(app))).alignment(Alignment::Left),
        area,
    );
}

/// Ponctuation portée par le mot en cours de lecture.
fn current_punctuation(app: &App) -> Option<&'static str> {
    let index = app.current_word()?;
    let sentence = app.doc().sentence(app.current_sentence())?;
    sentence.words.get(index)?.punctuation.map(|p| p.label())
}

fn render_panel(frame: &mut Frame, app: &App, area: Rect) {
    let (title, lines) = match app.panel {
        Panel::Contents => ("Sommaire", contents_lines(app)),
        Panel::Bookmarks => ("Signets", bookmark_lines(app)),
        Panel::None => return,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FAINT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    // Garde la ligne sélectionnée dans le cadre.
    let height = area.height.saturating_sub(2) as usize;
    let scroll = app.selection.saturating_sub(height.saturating_sub(1)) as u16;

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        area,
    );
}

fn contents_lines(app: &App) -> Vec<Line<'static>> {
    let doc = app.doc();
    let current = doc
        .section_of_sentence(app.current_sentence())
        .map(|s| s.id);

    // Même liste que celle qu'interroge la sélection : les index concordent.
    app.navigable_sections()
        .into_iter()
        .filter_map(|id| doc.section(id))
        .enumerate()
        .map(|(i, section)| {
            let indent = "  ".repeat(section.level.saturating_sub(1) as usize);
            let selected = i == app.selection;
            let active = current == Some(section.id);
            let style = match (selected, active) {
                (true, _) => Style::default().bg(ACCENT).fg(Color::Black),
                (false, true) => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(NORMAL),
            };
            Line::from(Span::styled(
                format!("{indent}{}", truncate(&section.title, 30)),
                style,
            ))
        })
        .collect()
}

fn bookmark_lines(app: &App) -> Vec<Line<'static>> {
    let items: Vec<_> = app.bookmarks.for_document(&app.source).collect();
    if items.is_empty() {
        return vec![Line::from(Span::styled(
            "Aucun signet. `b` pour en poser un.",
            Style::default().fg(DIM),
        ))];
    }

    let mut lines = Vec::new();
    for (i, bookmark) in items.iter().enumerate() {
        let selected = i == app.selection;
        let style = if selected {
            Style::default().bg(ACCENT).fg(Color::Black)
        } else {
            Style::default().fg(NORMAL)
        };
        lines.push(Line::from(Span::styled(
            truncate(&bookmark.snippet, 34),
            style,
        )));
        lines.push(Line::from(Span::styled(
            format!("  {} · {}", bookmark.timestamp(), truncate(&bookmark.section_title, 16)),
            Style::default().fg(FAINT),
        )));
    }
    lines
}

fn render_help(
    frame: &mut Frame,
    area: Rect,
    entries: &[(&str, &str)],
    title: &str,
) {
    // La boîte se règle sur son contenu : une largeur fixe rognait les
    // descriptions les plus longues, en plein milieu d'un mot.
    let needed = entries
        .iter()
        .map(|(keys, description)| keys.chars().count().max(12) + description.chars().count() + 4)
        .max()
        .unwrap_or(40) as u16;
    let width = needed.max(34).min(area.width.saturating_sub(4));
    let height = (entries.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let lines: Vec<Line> = entries
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(
                    format!(" {keys:<12}"),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*description, Style::default().fg(NORMAL)),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Formate des secondes en `M:SS` ou `H:MM:SS`.
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// ------------------------------------------------------- écran de configuration

/// Dessine l'écran de configuration, qui remplace la vue de lecture.
///
/// Deux pages, jamais côte à côte : le menu, puis la rubrique ouverte. On sait
/// toujours où l'on est, et les flèches n'ont qu'un sens à la fois.
fn render_config(frame: &mut Frame, app: &App, area: Rect) {
    match app.screen.page {
        Page::Menu => render_config_menu(frame, app, area),
        Page::Detail => render_config_detail(frame, app, area),
    }
}

/// La liste des rubriques, seule à l'écran.
fn render_config_menu(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // titre
            Constraint::Min(6),    // rubriques
            Constraint::Length(2), // aide et messages
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  Configuration",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   moteur : {}", app.engine_name()),
                    Style::default().fg(DIM),
                ),
            ]),
        ]),
        rows[0],
    );

    // Une colonne de valeurs alignée sur le libellé le plus long : l'œil
    // descend la liste sans avoir à chercher où commence chaque état.
    let label_width = Section::ALL
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(16)
        + 4;

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, section) in Section::ALL.iter().enumerate() {
        let selected = i == app.screen.section;
        let style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(NORMAL)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("   {} ", if selected { "▸" } else { " " }),
                style,
            ),
            Span::styled(
                format!("{} ", i + 1),
                Style::default()
                    .fg(if selected { ACCENT } else { FAINT })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {:<label_width$}", section.label()), style),
            Span::styled(
                section.summary(&app.config),
                Style::default().fg(if selected { NORMAL } else { FAINT }),
            ),
        ]));
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines), rows[1]);
    render_config_footer(frame, app, rows[2]);
}

/// Une rubrique ouverte, en pleine largeur.
fn render_config_detail(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // fil d'Ariane
            Constraint::Min(4),    // contenu
            Constraint::Length(2), // aide et messages
        ])
        .split(area);

    let section = app.screen.current_section();
    // Le chevron rappelle d'où l'on vient, et que Échap y ramène.
    let mut head = vec![
        Span::styled("  ‹ ", Style::default().fg(FAINT)),
        Span::styled(
            format!("{} {}", app.screen.section + 1, section.label()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 60 {
        head.push(Span::styled(
            format!("   moteur : {}", app.engine_name()),
            Style::default().fg(DIM),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(head),
            Line::from(Span::styled(
                format!("  {}", "─".repeat((area.width as usize).saturating_sub(4))),
                Style::default().fg(FAINT),
            )),
        ]),
        rows[0],
    );

    match section {
        Section::Models => render_model_list(frame, app, rows[1]),
        other => render_config_fields(frame, app, other, rows[1]),
    }
    render_config_footer(frame, app, rows[2]);
}

fn render_config_fields(frame: &mut Frame, app: &App, _section: Section, area: Rect) {
    let volume = app.player.volume();
    let mut lines: Vec<Line> = Vec::new();

    let fields = app.screen.visible_fields(&app.config);
    if fields.is_empty() {
        lines.push(Line::from(Span::styled(
            "    Aucun réglage de cette rubrique n'agit sur le modèle chargé.",
            Style::default().fg(NORMAL),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "    a  pour les afficher quand même",
            Style::default().fg(FAINT),
        )));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Une rubrique plus haute que la fenêtre défile autour du réglage courant,
    // au lieu d'être tronquée sans que rien ne le signale.
    let reserved = 5; // ligne vide, mention des réglages masqués, explication
    let room = (area.height as usize).saturating_sub(reserved).max(3);
    let start = if fields.len() <= room {
        0
    } else {
        app.screen
            .row
            .saturating_sub(room / 2)
            .min(fields.len() - room)
    };
    let end = (start + room).min(fields.len());
    if start > 0 {
        lines.push(Line::from(Span::styled(
            format!("    ↑ {start} de plus"),
            Style::default().fg(FAINT),
        )));
    }

    for (i, field) in fields.iter().enumerate().take(end).skip(start) {
        let selected = i == app.screen.row;
        let marker = if selected { "▸" } else { " " };
        let label_style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(NORMAL)
        };

        // Un champ en cours de saisie montre son tampon et un curseur.
        let value = match (&app.screen.editing, selected) {
            (Some(buffer), true) => format!("{buffer}▏"),
            _ => field.value(&app.config, volume),
        };
        let value_style = if app.screen.editing.is_some() && selected {
            Style::default().bg(ACCENT).fg(Color::Black)
        } else if selected {
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BRIGHT)
        };

        // Un réglage que la famille chargée ignore est marqué comme tel,
        // plutôt que de laisser croire qu'il agit.
        let ignored = field.is_ignored(&app.config);

        let mut spans = vec![
            Span::styled(format!("  {marker} {:<24}", field.label()), label_style),
            Span::styled(value, value_style),
        ];
        if ignored {
            spans.push(Span::styled(
                "  (ignoré par ce modèle)",
                Style::default().fg(FAINT),
            ));
        }
        lines.push(Line::from(spans));
    }

    if end < fields.len() {
        lines.push(Line::from(Span::styled(
            format!("    ↓ {} de plus", fields.len() - end),
            Style::default().fg(FAINT),
        )));
    }

    // Ce qui a été retiré de la vue doit rester récupérable d'un geste.
    let hidden = app.screen.hidden_count(&app.config);
    if hidden > 0 {
        lines.push(Line::from(""));
        let pluriel = if hidden > 1 { "s" } else { "" };
        // Sur une colonne étroite, la phrase complète serait coupée en plein
        // milieu : elle se réduit alors à ce qui compte.
        let message = if area.width >= 52 {
            format!("    {hidden} réglage{pluriel} sans effet sur ce modèle · a pour les voir")
        } else {
            format!("    {hidden} sans effet · a")
        };
        lines.push(Line::from(Span::styled(message, Style::default().fg(FAINT))));
    }

    lines.push(Line::from(""));

    if let Some(field) = fields.get(app.screen.row) {
        // L'explication du champ sélectionné, plutôt qu'un manuel séparé.
        let width = (area.width as usize).saturating_sub(6);
        for line in fold(&field.hint_for(&app.config), width) {
            lines.push(Line::from(Span::styled(
                format!("    {line}"),
                Style::default().fg(DIM),
            )));
        }
        if field.is_text(&app.config) && app.screen.editing.is_none() {
            lines.push(Line::from(Span::styled(
                "    Entrée pour éditer",
                Style::default().fg(FAINT),
            )));
        }
    }

    // Pas de coupe automatique : elle rognerait l'indentation des colonnes, et
    // les explications sont déjà pliées à la bonne largeur.
    frame.render_widget(Paragraph::new(lines), area);
}

/// Coupe un texte trop long, en signalant la coupe.
fn ellipsis(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
}

/// Plie un texte à la largeur donnée, sur les espaces.
fn fold(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let cost = word.chars().count() + usize::from(!current.is_empty());
        if current.chars().count() + cost > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        } else if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn render_model_list(frame: &mut Frame, app: &App, area: Rect) {
    let packages = models::available();
    let mut lines: Vec<Line> = Vec::new();

    if packages.is_empty() {
        lines.push(Line::from(Span::styled(
            " Aucun modèle disponible.",
            Style::default().fg(NORMAL).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Ce binaire a été compilé sans audio.cpp ; la lecture",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(Span::styled(
            " utilise la voix du système. Pour un vrai modèle :",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   cargo build --release --features audio-cpp",
            Style::default().fg(ACCENT),
        )));
        // `trim` rognerait l'indentation : la valeur du champ sélectionné se
    // décalerait de deux colonnes par rapport aux autres.
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    let active = app.config.engine.model_path.as_str();
    // La colonne des noms se règle sur le plus long, sans jamais écraser les
    // badges : un nom trop long est coupé plutôt que de les repousser.
    let longest = packages
        .iter()
        .map(|p| p.display_name.chars().count())
        .max()
        .unwrap_or(20);
    let name_width = longest.min((area.width as usize).saturating_sub(26)).max(12);

    for (i, package) in packages.iter().enumerate() {
        let selected = i == app.screen.row;
        let installed = models::is_installed(package);
        let truncated = models::is_truncated(package);
        let is_active = models::install_path(package)
            .is_some_and(|p| p.to_string_lossy() == active);

        let (badge, badge_style) = if truncated {
            ("⚠ tronqué ", Style::default().fg(Color::Black).bg(ACCENT))
        } else if is_active {
            ("● utilisé ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        } else if installed {
            ("○ installé", Style::default().fg(NORMAL))
        } else {
            ("  à venir ", Style::default().fg(FAINT))
        };

        let name_style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BRIGHT)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "  {} {:<width$}",
                    if selected { "▸" } else { " " },
                    ellipsis(package.display_name, name_width),
                    width = name_width + 1
                ),
                name_style,
            ),
            Span::styled(badge, badge_style),
            Span::styled(
                format!("  {}", models::human_size(package.bytes)),
                Style::default().fg(DIM),
            ),
        ]));

        if selected {
            lines.push(Line::from(Span::styled(
                format!("      {}", package.description),
                Style::default().fg(DIM),
            )));
            if truncated {
                lines.push(Line::from(Span::styled(
                    format!(
                        "     Fichier incomplet : {} au lieu de {}. Entrée pour réinstaller.",
                        models::human_size(models::installed_size(package).unwrap_or(0)),
                        models::human_size(package.bytes)
                    ),
                    Style::default().fg(ACCENT),
                )));
            }
            if package.recommended && !installed {
                lines.push(Line::from(Span::styled(
                    "      Recommandé pour commencer.",
                    Style::default().fg(ACCENT),
                )));
            }
            lines.push(Line::from(Span::styled(
                format!("      {}", package.languages_label),
                Style::default().fg(FAINT),
            )));
        }
    }

    // Barre de progression du téléchargement en cours.
    if let Some(download) = app.screen.download.as_ref() {
        if let models::Progress::Running { received, total } = download.progress() {
            let width = (area.width as usize).saturating_sub(8).min(48);
            let ratio = if total > 0 {
                (received as f64 / total as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let filled = (width as f64 * ratio).round() as usize;
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("█".repeat(filled), Style::default().fg(ACCENT)),
                Span::styled(
                    "─".repeat(width.saturating_sub(filled)),
                    Style::default().fg(FAINT),
                ),
                Span::styled(
                    format!(
                        "  {} / {}",
                        models::human_size(received),
                        models::human_size(total)
                    ),
                    Style::default().fg(NORMAL),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("  {} — d pour interrompre", download.package.display_name),
                Style::default().fg(DIM),
            )));
        }
    } else if let Some(free) = models::free_space() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Espace disque libre : {}", models::human_size(free)),
            Style::default().fg(FAINT),
        )));
    }

    // `trim` rognerait l'indentation : la valeur du champ sélectionné se
    // décalerait de deux colonnes par rapport aux autres.
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_config_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Sans document ouvert, « Échap retour » n'aurait nulle part où revenir.
    let exit = if app.doc().is_empty() { "q quitter" } else { "Échap retour" };
    // Le rappel ne montre que ce qui agit à cet instant : dans la colonne des
    // rubriques, « ←→ valeur » n'aurait rien à modifier.
    let keys = if app.screen.editing.is_some() {
        "Entrée valider · Échap annuler".to_string()
    } else if app.screen.page == Page::Menu {
        format!("↑↓ choisir · Entrée ouvrir · 1…5 accès direct · t écouter · {exit}")
    } else if app.screen.current_section() == Section::Models {
        "↑↓ modèle · Entrée installer/choisir · t écouter · d supprimer · Échap menu".to_string()
    } else {
        "↑↓ réglage · ←→ valeur · Entrée éditer · a tout voir · w enregistrer · Échap menu"
            .to_string()
    };

    let mut lines = vec![Line::from(Span::styled(
        format!(" {keys}"),
        Style::default().fg(FAINT),
    ))];

    // L'aperçu passe devant : c'est le seul message qui décrit une action en
    // train de se produire, et il dure le temps qu'on l'écoute.
    let message = if let Some(preview) = app.preview_status() {
        preview
    } else if app.reference_missing() {
        "⚠ ce modèle ne parle qu'en clonant : renseignez un WAV dans « Voix »".to_string()
    } else if !app.screen.notice.is_empty() {
        app.screen.notice.clone()
    } else if !app.status().is_empty() {
        app.status().to_string()
    } else {
        String::new()
    };
    if !message.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {message}"),
            Style::default().fg(ACCENT),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        layout_tracked(&source, marks, width, indent, Style::default(), None)
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
        let (lines, cursor) = layout_tracked(&source, marks, 12, 0, Style::default(), None);
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
        use crate::tui::search::Search;
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
        use crate::tui::search::Search;
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
        let mut app = crate::tui::app::tests::app_reading("Une phrase.\n");
        app.search_open();
        app.search_push('r');
        app.search_push('o');
        let line = text_of(&status_spans(&app));
        assert!(line.contains("ro"), "la saisie doit se voir : {line:?}");
        assert!(line.contains("Échap"), "la sortie doit être dite : {line:?}");
    }

    #[test]
    fn the_status_line_keeps_saying_that_the_view_is_frozen() {
        let mut app = crate::tui::app::tests::app_reading("Une phrase.\n\nUne autre.\n");
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
}

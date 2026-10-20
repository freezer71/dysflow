//! Le texte lu : préfixes de blocs, styles des mots, repli des lignes autour du curseur.

use super::*;

/// Ce qu'un mot apporte à la ligne où le fichier l'a écrit.
///
/// Le texte, lui, vient du document : c'est sa composition qui est à l'écran,
/// et les mots ne font que la styler, dans l'ordre.
pub(super) struct Mark {
    pub(super) style: Style,
    /// Vrai pour le mot en train d'être prononcé.
    pub(super) cursor: bool,
}

pub(super) fn render_reader(frame: &mut Frame, app: &App, area: Rect) {
    // Une ligne trop large se relit mal : on perd le début de la suivante.
    let viewport = reading_viewport(area, app.config.reading.line_width as usize);
    let width = viewport.width as usize;
    if width < 8 || viewport.height == 0 {
        return;
    }

    let doc = app.doc();
    if doc.is_empty() {
        let message = if app.input_complete() {
            crate::tr!("Nothing readable in the input.", "Aucun contenu lisible dans l'entrée.")
        } else {
            crate::tr!("Waiting for the stream…", "En attente du flux…")
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
        rows: viewport.height as usize,
        ..ReaderFrame::new(doc, width)
    });

    // Le point d'ancrage reste au tiers supérieur : on voit ce qui vient.
    let scroll = view.anchor.saturating_sub(viewport.height as usize / 3);

    frame.render_widget(
        Paragraph::new(view.lines).scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        viewport,
    );
}

/// Ce qui ouvre les lignes d'un bloc : son décalage, et sa marque.
pub(super) struct Prefix {
    pub(super) indent: usize,
    pub(super) marker: Option<(String, Style)>,
    /// Vrai quand la marque ouvre chaque ligne, et non la première seule.
    pub(super) every_line: bool,
}

impl Prefix {
    /// Colonnes qu'occupe la marque, espace compris.
    pub(super) fn width(&self) -> usize {
        self.marker
            .as_ref()
            .map_or(0, |(text, _)| columns(text) + 1)
    }

    /// Décalage des lignes qui prolongent la première.
    ///
    /// La suite d'un item pend sous son texte, jamais sous sa puce : la liste
    /// garde ainsi sa colonne. Une citation, elle, reprend son filet.
    pub(super) fn hanging(&self) -> usize {
        if self.every_line {
            self.indent
        } else {
            self.indent + self.width()
        }
    }
}

/// Indentation et marqueur visuel d'un bloc.
pub(super) fn block_prefix(kind: &BlockKind) -> Prefix {
    let plain = |indent| Prefix { indent, marker: None, every_line: false };
    match kind {
        BlockKind::Heading { .. } | BlockKind::Paragraph => plain(0),
        BlockKind::ListItem { depth, ordinal } => Prefix {
            indent: 2 + *depth as usize * 2,
            marker: Some((
                match ordinal {
                    Some(n) => format!("{n}."),
                    None => "•".to_string(),
                },
                Style::default().fg(ACCENT),
            )),
            every_line: false,
        },
        // Le filet de la citation tient toute sa hauteur : à la première ligne
        // seule, la suivante se lirait comme de la prose décalée.
        BlockKind::Quote => Prefix {
            indent: 2,
            marker: Some(("│".to_string(), Style::default().fg(FAINT))),
            every_line: true,
        },
        // Code et tableaux ne passent pas par ici : ils portent leur propre
        // filet, ligne à ligne.
        BlockKind::Code { .. } | BlockKind::TableRow { .. } => plain(2),
    }
}

/// Ajoute au mot la mise en valeur qu'il porte dans le fichier.
///
/// Jamais de fond : le mot prononcé reste le seul de l'écran à en porter un.
/// Et jamais de couleur hors du bloc lu, qui doit s'effacer en entier — la
/// graisse et le soulignement, eux, restent lisibles sans faire de trou dans
/// la page.
pub(super) fn emphasised(style: Style, emphasis: &Emphasis, in_current_block: bool) -> Style {
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

pub(super) fn word_style(
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
pub(super) fn reading_viewport(area: Rect, max: usize) -> Rect {
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
pub(super) fn space_out(lines: Vec<Line<'static>>, spacing: usize) -> Vec<Line<'static>> {
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
pub(super) fn ruled(line: Line<'static>, width: usize, background: Color) -> Line<'static> {
    let mut spans = line.spans;
    let used: usize = spans.iter().map(|s| columns(&s.content)).sum();
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
/// Colonnes qu'un texte occupe à l'écran.
///
/// Un idéogramme en prend deux, un caractère combinant aucune : compter les
/// caractères faisait déborder le chinois et le coréen, que le catalogue
/// annonce pourtant, et le mot prononcé pouvait sortir de l'écran.
pub(super) fn columns(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

pub(super) fn cells(source: &str) -> Vec<(usize, String)> {
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

/// Ouvre une ligne : son décalage, puis la marque du bloc s'il en porte une.
pub(super) fn open_line(prefix: &Prefix, marked: bool) -> (Vec<Span<'static>>, usize) {
    let mut spans = Vec::new();
    let mut used = prefix.indent;
    if prefix.indent > 0 {
        spans.push(Span::raw(" ".repeat(prefix.indent)));
    }
    // L'espace qui suit la marque vient de l'écart du premier mot, comme
    // entre deux mots : la marque compte pour un mot de plus.
    if let (true, Some((text, style))) = (marked, prefix.marker.as_ref()) {
        used += columns(text);
        spans.push(Span::styled(text.clone(), *style));
    }
    (spans, used)
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
pub(super) fn layout_tracked(
    source: &[String],
    marks: Vec<Mark>,
    width: usize,
    prefix: &Prefix,
    base: Style,
) -> (Vec<Line<'static>>, Option<usize>) {
    if width == 0 {
        return (Vec::new(), None);
    }
    // La marque du bloc occupe le début de sa ligne : elle compte dans ce que
    // cette ligne peut tenir.
    let head = prefix.width();
    let recomposed: Vec<String>;
    let source: &[String] = if source.iter().enumerate().all(|(n, line)| {
        let marked = usize::from(n == 0 || prefix.every_line) * head;
        prefix.indent + marked + columns(line) <= width
    }) {
        source
    } else {
        recomposed = vec![source.join(" ")];
        &recomposed
    };

    let mut words = marks.into_iter();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_line = None;

    for (n, source_line) in source.iter().enumerate() {
        // La marque du bloc — puce, filet de citation — ouvre la ligne et se
        // comporte ensuite comme un mot de plus.
        let (mut spans, mut used) = open_line(prefix, n == 0 || prefix.every_line);
        let mut placed = used > prefix.indent;

        for (written, text) in cells(source_line) {
            // L'écart du fichier ouvre la ligne — c'est son indentation — et
            // sépare ensuite deux mots sans jamais les coller.
            let mut gap = if placed { written.max(1) } else { written };
            let len = columns(&text);
            if placed && used + gap + len > width {
                lines.push(Line::from(std::mem::take(&mut spans)));
                let (opened, at) = open_line(prefix, prefix.every_line);
                spans = opened;
                used = at;
                placed = at > prefix.indent;
                // Ce qui prolonge une ligne pend sous son texte, jamais sous
                // sa puce : la liste garde ainsi sa colonne.
                let pad = prefix.hanging().saturating_sub(used);
                if pad > 0 {
                    spans.push(Span::raw(" ".repeat(pad)));
                    used += pad;
                }
                gap = usize::from(placed);
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
        lines.push(if placed || used > prefix.indent {
            Line::from(spans)
        } else {
            Line::default()
        });
    }
    (lines, cursor_line)
}

/// Lignes à afficher, et index de celle qui ouvre le bloc en cours de lecture.
pub(super) struct ReaderView {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) anchor: usize,
}

/// Style d'un bloc affiché tel qu'il est écrit : texte, puis filet.
pub(super) fn fixed_style(kind: &BlockKind, current: bool) -> (Style, Style) {
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
pub(super) fn gutters(kind: &BlockKind) -> (&'static str, &'static str) {
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
pub(super) fn fixed_lines(
    source: &[String],
    width: usize,
    kind: &BlockKind,
    current: bool,
) -> Vec<Line<'static>> {
    let (style, gutter) = fixed_style(kind, current);
    let (opening, continuation) = gutters(kind);
    let room = width.saturating_sub(columns(opening)).max(1);

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
pub(super) struct ReaderFrame<'a> {
    pub(super) doc: &'a crate::doc::Document,
    /// Bloc autour duquel dessiner.
    pub(super) anchor_block: usize,
    pub(super) current_sentence: usize,
    pub(super) current_word: Option<usize>,
    pub(super) width: usize,
    pub(super) style: ReadingStyle,
    /// Recherche en cours, dont les occurrences se signalent dans le texte.
    pub(super) search: Option<&'a crate::app::search::Search>,
    /// Lignes visibles sous l'ancre : au-delà, rien ne se met en page. Zéro
    /// ne borne rien.
    pub(super) rows: usize,
}

impl<'a> ReaderFrame<'a> {
    pub(super) fn new(doc: &'a crate::doc::Document, width: usize) -> Self {
        Self {
            doc,
            anchor_block: 0,
            current_sentence: 0,
            current_word: None,
            width,
            style: ReadingStyle { spacing: 0, ruler: false },
            search: None,
            rows: 0,
        }
    }
}

/// Met le document en lignes autour du curseur.
///
/// Séparée du rendu pour être vérifiable : c'est ici que se décident le repli,
/// l'interlignage et la règle de lecture.
pub(super) fn document_lines(frame: ReaderFrame<'_>) -> ReaderView {
    let ReaderFrame {
        doc,
        anchor_block,
        current_sentence,
        current_word,
        width,
        style,
        search,
        rows,
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
        // L'ancre se tient au tiers de l'écran : ce qui passe sa hauteur
        // sous elle ne se verrait pas, inutile de le mettre en page.
        if rows > 0 && block.id > anchor_block && lines.len() >= anchor + rows {
            break;
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

        let prefix = block_prefix(&block.kind);
        let mut marks: Vec<Mark> = Vec::new();

        for id in block.sentences.0..block.sentences.1 {
            let Some(sentence) = doc.sentence(id) else { continue };
            let is_current_sentence = id == current_sentence;
            // Seules les phrases trouvées ont des mots à souligner : les
            // autres ne passent pas chaque mot à la comparaison.
            let search = search.filter(|s| s.contains_sentence(id));
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
            &prefix,
            word_style(&block.kind, &Emphasis::default(), is_current_block, false, false),
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

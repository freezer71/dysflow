//! La zone de texte : composition mise en cache, règle, suivi de la voix, clic.
//!
//! Chaque bloc garde sa composition tant que rien n'a changé pour lui. Un bloc
//! jamais vu a une hauteur estimée, prise sur son nombre de caractères ; il est
//! composé quand il approche de la fenêtre, et sa hauteur réelle remplace
//! l'estimation sans que ce qui est à l'écran bouge. Le coût d'une trame dépend
//! de la fenêtre, pas du document.

use std::ops::Range;
use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Galley, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};

use super::text::{self, Cursor, Mark, Run, Tone};
use super::theme::Palette;
use super::{flow, fonts};
use crate::app::search::Search;
use crate::app::App;
use crate::doc::{Block, BlockKind, Document};

/// Ce dont dépend la composition d'un bloc : quand ça change, on recompose.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    /// Largeur de colonne, style, thème, recherche : tout ce qui vaut pour le
    /// document entier, résumé par un compteur.
    generation: u64,
    current_block: bool,
    /// Phrase et mot lus, s'ils sont dans ce bloc.
    sentence: Option<usize>,
    word: Option<usize>,
    /// Nombre de phrases : un flux peut en ajouter au dernier bloc.
    sentences: usize,
}

struct Slot {
    key: Option<Key>,
    galley: Option<Arc<Galley>>,
    /// Puce ou rang d'un item de liste, composé à part pour que la suite du
    /// texte pende sous le texte et non sous la puce.
    marker: Option<Arc<Galley>>,
    height: f32,
    /// Plages de caractères → phrase, pour le clic.
    spans: Vec<(Range<usize>, usize)>,
    /// Plage du mot prononcé, en caractères, pour la règle et le suivi.
    cursor: Option<Range<usize>>,
}

impl Slot {
    fn estimated(height: f32) -> Self {
        Self { key: None, galley: None, marker: None, height, spans: Vec::new(), cursor: None }
    }
}

/// Mise en page d'une trame, dérivée de la configuration et de la fenêtre.
struct Metrics {
    column_x: f32,
    column_w: f32,
    size: f32,
    line_h: f32,
    /// Espace sous un bloc.
    gap: f32,
    font: String,
    ruler: bool,
}

/// Ce qu'une trame de composition rend au défilement.
struct Pass {
    tops: Vec<f32>,
    total: f32,
    /// Haut de la ligne du mot prononcé, en coordonnées du contenu.
    spoken: Option<f32>,
}

pub struct Reader {
    slots: Vec<Slot>,
    /// Résumé de ce qui vaut pour tous les blocs ; incrémenté quand ça change.
    generation: u64,
    fingerprint: Option<(i32, String, String, i32, i32, bool, Option<String>)>,
    /// Décalage de défilement tenu par la vue.
    offset: f32,
    /// Décalage posé à la dernière trame ; s'il diffère de celui qu'on relit,
    /// quelqu'un a défilé.
    set: Option<f32>,
    tops: Vec<f32>,
    total: f32,
    /// Pages demandées au clavier, en signe, à servir à la prochaine trame.
    page: i32,
}

/// Blocs composés au plus par trame au-delà de ceux qui sont visibles.
const BUDGET: usize = 24;

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            generation: 0,
            fingerprint: None,
            offset: 0.0,
            set: None,
            tops: Vec::new(),
            total: 0.0,
            page: 0,
        }
    }

    /// Oublie toute composition : un autre document arrive.
    pub fn reset(&mut self) {
        self.slots.clear();
        self.offset = 0.0;
        self.set = None;
        self.tops.clear();
        self.total = 0.0;
        self.page = 0;
    }

    /// Défile d'une page dans ce sens à la prochaine trame, et épingle la vue.
    pub fn page(&mut self, direction: i32) {
        self.page += direction;
    }

    pub fn show(&mut self, ui: &mut Ui, app: &mut App, palette: &Palette) {
        if app.doc().blocks.is_empty() {
            let message = if app.input_complete() {
                "Aucun contenu lisible dans l'entrée."
            } else {
                "En attente du flux…"
            };
            ui.add_space(24.0);
            ui.label(egui::RichText::new(message).color(palette.dim));
            return;
        }

        let metrics = self.metrics(ui, app);
        self.bump_generation(&metrics, app);
        self.fit_slots(app.doc(), &metrics);

        let viewport_h = ui.available_height();
        if self.page != 0 {
            let max = (self.total - viewport_h).max(0.0);
            self.offset = (self.offset + self.page as f32 * viewport_h * 0.9).clamp(0.0, max);
            app.view.pin(flow::block_at(&self.tops, self.offset));
            self.page = 0;
            self.set = Some(self.offset);
        }
        let following = app.view.is_following();
        let mut area = egui::ScrollArea::vertical()
            .id_salt("lecture")
            .auto_shrink([false, false]);
        if following || self.set.is_some() {
            area = area.vertical_scroll_offset(self.offset);
            self.set = Some(self.offset);
        }

        let cursor = Cursor { sentence: app.current_sentence(), word: app.current_word() };
        let search = app.search().cloned();
        let doc = app.doc();
        let mut clicked: Option<usize> = None;
        let output = area.show_viewport(ui, |ui, viewport| {
            let pass = self.compose(
                ui,
                doc,
                cursor,
                search.as_ref(),
                &metrics,
                palette,
                viewport,
                following,
            );
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), pass.total), Sense::click());
            self.paint(ui, doc, &pass, &metrics, palette, rect, viewport);
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    clicked = self.sentence_at(doc, &pass, &metrics, rect, pos);
                }
            }
            pass
        });
        let pass = output.inner;
        self.tops = pass.tops;
        self.total = pass.total;

        // Quelqu'un a défilé : la vue s'épingle sur le bloc en haut de l'écran.
        let read = output.state.offset.y;
        if let Some(set) = self.set {
            if flow::user_scrolled(set, read) {
                app.view.pin(flow::block_at(&self.tops, read));
                self.offset = read;
            }
        }
        if app.view.is_following() {
            if let Some(row_top) = pass.spoken {
                let target = flow::follow_target(row_top, viewport_h, self.total);
                self.offset = flow::approach(self.offset, target, viewport_h);
            }
        } else {
            self.offset = read;
            self.set = None;
        }
        if let Some(sentence) = clicked {
            app.seek_to(sentence);
        }
        if self.slots.iter().any(|s| s.galley.is_none()) {
            ui.ctx().request_repaint();
        }
    }

    fn metrics(&self, ui: &Ui, app: &App) -> Metrics {
        let gui = &app.config.gui;
        let size = gui.font_size();
        let line_h = size * gui.line_height();
        let margin = size;
        let usable = (ui.available_width() - 2.0 * margin - 16.0).max(80.0);
        let column_w = match app.config.reading.line_width {
            0 => usable,
            chars => usable.min(chars as f32 * size * 0.5),
        };
        Metrics {
            column_x: margin + (usable - column_w) / 2.0,
            column_w,
            size,
            line_h,
            gap: line_h * 0.7,
            font: gui.font.clone(),
            ruler: app.config.reading.reading_ruler,
        }
    }

    /// Change de génération quand ce qui vaut pour tous les blocs a changé.
    fn bump_generation(&mut self, metrics: &Metrics, app: &App) {
        let fingerprint = (
            metrics.column_w.round() as i32,
            metrics.font.clone(),
            app.config.gui.theme.clone(),
            (metrics.size * 10.0) as i32,
            (metrics.line_h * 10.0) as i32,
            metrics.ruler,
            app.search().map(|s| s.query.clone()),
        );
        if self.fingerprint.as_ref() != Some(&fingerprint) {
            self.generation += 1;
            self.fingerprint = Some(fingerprint);
        }
    }

    /// Autant d'emplacements que de blocs : un flux en ajoute, un autre
    /// document en retire.
    fn fit_slots(&mut self, doc: &Document, metrics: &Metrics) {
        self.slots.truncate(doc.blocks.len());
        let chars_per_line = metrics.column_w / (metrics.size * 0.5);
        for block in &doc.blocks[self.slots.len()..] {
            let chars = block.text.chars().count();
            self.slots.push(Slot::estimated(flow::estimated_height(
                chars,
                chars_per_line,
                metrics.line_h,
                metrics.gap,
            )));
        }
    }

    /// Compose ce qui approche de la fenêtre, avec un budget, et rend les
    /// positions de tous les blocs.
    #[allow(clippy::too_many_arguments)]
    fn compose(
        &mut self,
        ui: &Ui,
        doc: &Document,
        cursor: Cursor,
        search: Option<&Search>,
        metrics: &Metrics,
        palette: &Palette,
        viewport: Rect,
        following: bool,
    ) -> Pass {
        let current_block = doc.sentence(cursor.sentence).map(|s| s.block_id);
        let near = viewport.expand2(Vec2::new(0.0, viewport.height()));
        let mut budget = BUDGET;
        let mut tops = Vec::with_capacity(doc.blocks.len());
        let mut y = 0.0;
        let mut spoken = None;

        for (i, block) in doc.blocks.iter().enumerate() {
            tops.push(y);
            let old = self.slots[i].height;
            let span = Rect::from_min_size(Pos2::new(0.0, y), Vec2::new(1.0, old));
            let is_current = current_block == Some(block.id);
            let key = Key {
                generation: self.generation,
                current_block: is_current,
                sentence: is_current.then_some(cursor.sentence),
                word: if is_current { cursor.word } else { None },
                sentences: block.sentences.1 - block.sentences.0,
            };
            let stale = self.slots[i].key.as_ref() != Some(&key);
            let visible = span.intersects(viewport);
            if stale && (visible || (span.intersects(near) && budget > 0)) {
                let glued = glued_to_next(doc, i);
                let slot = layout_block(
                    ui, doc, block, cursor, search, metrics, palette, is_current, glued,
                );
                self.slots[i] = Slot { key: Some(key), ..slot };
                if !visible {
                    budget -= 1;
                }
                // Un bloc au-dessus de la vue qui change de hauteur ne doit
                // pas déplacer ce qu'on lit.
                if !following {
                    self.offset = flow::absorb(self.offset, y, old, self.slots[i].height);
                }
            }
            if is_current {
                if let (Some(galley), Some(range)) = (&self.slots[i].galley, &self.slots[i].cursor) {
                    if let Some(rect) = row_of(galley, range.start) {
                        spoken = Some(y + rect.min.y);
                    }
                }
            }
            y += self.slots[i].height;
        }
        Pass { tops, total: y, spoken }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        ui: &Ui,
        doc: &Document,
        pass: &Pass,
        metrics: &Metrics,
        palette: &Palette,
        rect: Rect,
        viewport: Rect,
    ) {
        let painter = ui.painter();
        let origin = rect.min;
        for (i, block) in doc.blocks.iter().enumerate() {
            let slot = &self.slots[i];
            let top = pass.tops[i];
            if top + slot.height < viewport.min.y || top > viewport.max.y {
                continue;
            }
            let Some(galley) = &slot.galley else { continue };
            let y = origin.y + top;
            let x0 = origin.x + metrics.column_x;
            let (indent, marker_w) = prefix_widths(block, slot, metrics);
            let x_text = x0 + indent + marker_w;
            let fixed = matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. });

            if fixed {
                let glued = glued_to_next(doc, i);
                let h = slot.height - if glued { 0.0 } else { metrics.gap };
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x0, y), Vec2::new(metrics.column_w, h)),
                    2.0,
                    palette.code_bg,
                );
                if matches!(block.kind, BlockKind::Code { .. }) {
                    let current = slot.key.as_ref().is_some_and(|k| k.current_block);
                    painter.vline(
                        x0 + 3.0,
                        y..=y + h,
                        Stroke::new(2.0, if current { palette.accent } else { palette.faint }),
                    );
                }
            } else if metrics.ruler {
                // La règle passe sous la ligne entière : c'est elle qu'on
                // retrouve en relevant les yeux, pas le mot seul.
                if let Some(range) = &slot.cursor {
                    if let Some(row) = row_of(galley, range.start) {
                        painter.rect_filled(
                            Rect::from_min_max(
                                Pos2::new(x0 - metrics.size * 0.4, y + row.min.y - 2.0),
                                Pos2::new(
                                    x0 + metrics.column_w + metrics.size * 0.4,
                                    y + row.max.y + 2.0,
                                ),
                            ),
                            4.0,
                            palette.ruler,
                        );
                    }
                }
            }

            if matches!(block.kind, BlockKind::Quote) {
                let h = galley.size().y;
                painter.vline(x0 + indent * 0.35, y..=y + h, Stroke::new(3.0, palette.faint));
            }
            if let Some(marker) = &slot.marker {
                painter.galley(Pos2::new(x0 + indent, y), marker.clone(), palette.accent);
            }
            painter.galley(Pos2::new(x_text, y), galley.clone(), palette.normal);

            // Un titre est souligné d'un filet : la hiérarchie se lit d'un
            // coup d'œil sans qu'aucun marqueur ne soit prononcé.
            if let BlockKind::Heading { level } = block.kind {
                if level <= 2 {
                    let chars = if level == 1 { 48.0 } else { 28.0 };
                    let w = metrics.column_w.min(chars * metrics.size * 0.5);
                    let rule_y = y + galley.size().y + 4.0;
                    painter.hline(
                        x_text..=x_text + w,
                        rule_y,
                        Stroke::new(1.5, if level == 1 { palette.accent } else { palette.faint }),
                    );
                }
            }
        }
    }

    /// La phrase sous le pointeur, s'il est sur un mot.
    fn sentence_at(
        &self,
        doc: &Document,
        pass: &Pass,
        metrics: &Metrics,
        rect: Rect,
        pos: Pos2,
    ) -> Option<usize> {
        let y = pos.y - rect.min.y;
        let i = flow::block_at(&pass.tops, y);
        let block = doc.blocks.get(i)?;
        let slot = self.slots.get(i)?;
        let galley = slot.galley.as_ref()?;
        if matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. }) {
            return Some(block.sentences.0);
        }
        let (indent, marker_w) = prefix_widths(block, slot, metrics);
        let local = Vec2::new(
            pos.x - (rect.min.x + metrics.column_x + indent + marker_w),
            y - pass.tops[i],
        );
        let index: usize = galley.cursor_from_pos(local).index.into();
        slot.spans
            .iter()
            .find(|(range, _)| range.contains(&index))
            .or_else(|| slot.spans.iter().rev().find(|(range, _)| range.start <= index))
            .map(|(_, sentence)| *sentence)
    }
}

/// Deux lignes d'un même tableau se tiennent : pas d'écart entre elles.
fn glued_to_next(doc: &Document, i: usize) -> bool {
    matches!(doc.blocks[i].kind, BlockKind::TableRow { .. })
        && doc
            .blocks
            .get(i + 1)
            .is_some_and(|n| matches!(n.kind, BlockKind::TableRow { .. }))
}

/// Retrait d'un bloc, en pixels.
fn indent_of(block: &Block, metrics: &Metrics) -> f32 {
    match block.kind {
        BlockKind::ListItem { depth, .. } => metrics.size * (1.0 + 1.2 * depth as f32),
        BlockKind::Quote => metrics.size,
        BlockKind::Code { .. } | BlockKind::TableRow { .. } => metrics.size * 0.8,
        _ => 0.0,
    }
}

/// Retrait et largeur de la marque d'un bloc, en pixels.
fn prefix_widths(block: &Block, slot: &Slot, metrics: &Metrics) -> (f32, f32) {
    let marker_w = slot
        .marker
        .as_ref()
        .map_or(0.0, |m| m.size().x + metrics.size * 0.4);
    (indent_of(block, metrics), marker_w)
}

/// Rangée du texte composé qui contient le caractère `index`.
fn row_of(galley: &Galley, index: usize) -> Option<Rect> {
    let mut seen = 0usize;
    for placed in &galley.rows {
        let count: usize = placed.char_count_including_newline().into();
        if index < seen + count {
            return Some(placed.rect());
        }
        seen += count;
    }
    galley.rows.last().map(|r| r.rect())
}

/// Compose un bloc : texte, marque, plages, hauteur.
#[allow(clippy::too_many_arguments)]
fn layout_block(
    ui: &Ui,
    doc: &Document,
    block: &Block,
    cursor: Cursor,
    search: Option<&Search>,
    metrics: &Metrics,
    palette: &Palette,
    current: bool,
    glued: bool,
) -> Slot {
    let gap = if glued { 0.0 } else { metrics.gap };
    if matches!(block.kind, BlockKind::Code { .. } | BlockKind::TableRow { .. }) {
        return layout_fixed(ui, block, metrics, palette, current, gap);
    }

    let marker = match &block.kind {
        BlockKind::ListItem { ordinal, .. } => {
            let text = match ordinal {
                Some(n) => format!("{n}."),
                None => "•".to_string(),
            };
            let color = if current { palette.accent } else { palette.faint };
            Some(ui.painter().layout_no_wrap(
                text,
                FontId::new(metrics.size, fonts::family(&metrics.font, false, false)),
                color,
            ))
        }
        _ => None,
    };
    let indent = indent_of(block, metrics);
    let marker_w = marker
        .as_ref()
        .map_or(0.0, |m| m.size().x + metrics.size * 0.4);
    let width = (metrics.column_w - indent - marker_w).max(40.0);

    let runs = text::runs(doc, block, cursor, search);
    let (job, mut spans, mut cursor_range) =
        job_for(&runs, true, width, &block.kind, metrics, palette);
    let mut galley = ui.painter().layout_job(job);
    // La page reste celle du fichier tant qu'elle tient : si une ligne a dû
    // se replier, le bloc est recomposé d'un seul tenant.
    let file_lines = runs.iter().filter(|r| r.newline).count().max(1);
    if galley.rows.len() > file_lines {
        let (job, s, c) = job_for(&runs, false, width, &block.kind, metrics, palette);
        galley = ui.painter().layout_job(job);
        spans = s;
        cursor_range = c;
    }

    let rule = match block.kind {
        BlockKind::Heading { level } if level <= 2 => 8.0,
        _ => 0.0,
    };
    let height =
        galley.size().y.max(marker.as_ref().map_or(0.0, |m| m.size().y)) + rule + gap;
    Slot { key: None, galley: Some(galley), marker, height, spans, cursor: cursor_range }
}

/// Un bloc montré tel qu'il est écrit : code, tableau.
fn layout_fixed(
    ui: &Ui,
    block: &Block,
    metrics: &Metrics,
    palette: &Palette,
    current: bool,
    gap: f32,
) -> Slot {
    let size = metrics.size * 0.85;
    let color = match block.kind {
        BlockKind::TableRow { header: true } if current => palette.bright,
        BlockKind::TableRow { header: true } => palette.normal,
        _ if current => palette.normal,
        _ => palette.faint,
    };
    let mut format = TextFormat::simple(FontId::monospace(size), color);
    format.line_height = Some(size * 1.4);
    let text = if block.display_lines.is_empty() {
        block.text.clone()
    } else {
        block.display_lines.join("\n")
    };
    let mut job = LayoutJob::single_section(text, format);
    job.wrap.max_width = (metrics.column_w - metrics.size * 1.2).max(40.0);
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y + metrics.size * 0.4 + gap;
    Slot { key: None, galley: Some(galley), marker: None, height, spans: Vec::new(), cursor: None }
}

/// Traduit les runs en sections de texte. Rend aussi les plages de caractères
/// de chaque phrase et celle du mot prononcé.
fn job_for(
    runs: &[Run],
    keep_lines: bool,
    width: f32,
    kind: &BlockKind,
    metrics: &Metrics,
    palette: &Palette,
) -> (LayoutJob, Vec<(Range<usize>, usize)>, Option<Range<usize>>) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    job.break_on_newline = true;
    let plain = format_for(
        &Mark { tone: Tone::Faded, emphasis: Default::default(), hit: false },
        kind,
        metrics,
        palette,
    );
    let mut spans = Vec::new();
    let mut cursor = None;
    let mut chars = 0usize;
    let mut first = true;
    for run in runs {
        let spacer = if first {
            if keep_lines { " ".repeat(run.gap) } else { String::new() }
        } else if run.newline {
            if keep_lines { format!("\n{}", " ".repeat(run.gap)) } else { " ".to_string() }
        } else {
            " ".repeat(run.gap.max(1))
        };
        first = false;
        if !spacer.is_empty() {
            job.append(&spacer, 0.0, plain.clone());
            chars += spacer.chars().count();
        }
        if run.text.is_empty() {
            continue;
        }
        let len = run.text.chars().count();
        job.append(&run.text, 0.0, format_for(&run.mark, kind, metrics, palette));
        if let Some(sentence) = run.sentence {
            spans.push((chars..chars + len, sentence));
        }
        if run.mark.tone == Tone::Cursor {
            cursor = Some(chars..chars + len);
        }
        chars += len;
    }
    (job, spans, cursor)
}

/// Le format d'un jeton : police, taille, couleur, fond, traits.
///
/// Jamais de changement de graisse pour le mot prononcé ni la phrase lue : dans
/// une police proportionnelle, la ligne entière se recomposerait à chaque mot.
fn format_for(mark: &Mark, kind: &BlockKind, metrics: &Metrics, palette: &Palette) -> TextFormat {
    let (size, heading) = match kind {
        BlockKind::Heading { level: 1 } => (metrics.size * 1.6, true),
        BlockKind::Heading { level: 2 } => (metrics.size * 1.35, true),
        BlockKind::Heading { .. } => (metrics.size * 1.15, true),
        _ => (metrics.size, false),
    };
    let strong = mark.emphasis.strong || heading;
    let italic = mark.emphasis.italic || matches!(kind, BlockKind::Quote);
    let family = if mark.emphasis.code {
        FontFamily::Monospace
    } else {
        fonts::family(&metrics.font, strong, italic)
    };
    let mut color = match mark.tone {
        Tone::Cursor => palette.on_accent,
        Tone::Sentence => palette.bright,
        Tone::Block => match kind {
            BlockKind::Heading { level: 1 } => palette.accent,
            BlockKind::Heading { .. } => palette.bright,
            BlockKind::Quote => palette.dim,
            _ => palette.normal,
        },
        Tone::Faded => palette.faint,
    };
    if matches!(mark.tone, Tone::Sentence | Tone::Block)
        && (mark.emphasis.link || mark.emphasis.code)
    {
        color = palette.technical;
    }
    let line = Stroke::new(1.0, color);
    TextFormat {
        font_id: FontId::new(size, family),
        line_height: Some(size * (metrics.line_h / metrics.size)),
        color,
        background: if mark.tone == Tone::Cursor { palette.accent } else { Color32::TRANSPARENT },
        expand_bg: 2.0,
        italics: italic && !fonts::has_italic(&metrics.font),
        underline: if mark.emphasis.link || mark.hit { line } else { Stroke::NONE },
        strikethrough: if mark.emphasis.struck { line } else { Stroke::NONE },
        valign: Align::BOTTOM,
        ..Default::default()
    }
}

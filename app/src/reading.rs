//! La vue de lecture : le document mis en lignes, le mot lu sur un fond
//! arrondi, la règle sous la ligne lue, et la vue qui suit la lecture.
//!
//! Le fil du lecteur publie un instantané ; une tâche GPUI le relit (16 ms
//! pendant la lecture, 250 ms sinon) et ne redessine que s'il a changé. La
//! mise en page (`layout::lay_out`) n'est refaite que si le document, la
//! largeur, la police, sa taille ou l'interligne changent ; les largeurs de
//! mots mesurées restent en cache d'une mise en page à l'autre.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dysflow::config::{READING_SIZE_DEFAULT, READING_SIZE_MAX, READING_SIZE_MIN};
use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::settings::SettingsView;
use dysflow::reader::view::{BlockKindView, DocView, Snapshot, WordView};
use gpui::{
    div, fill, font, point, px, relative, size, App, Bounds, ContentMask, Context, DispatchPhase,
    Element, ElementId, Entity, Font, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent, ParentElement, Pixels,
    Point, Render, ScrollDelta, ScrollEvent, ShapedLine, SharedString, StrikethroughStyle, Style,
    Styled, Task, TextAlign, TextRun, Window, WindowTextSystem,
};

use crate::layout::{lay_out, Layout, Line, Measure, WordBox};
use crate::session::Session;

/// Couleurs provisoires, en un seul endroit : le thème arrive plus tard.
mod colors {
    use gpui::{rgb, rgba, Hsla};

    pub fn background() -> Hsla {
        rgb(0xfbf8f1).into()
    }
    pub fn text() -> Hsla {
        rgb(0x1f1d1a).into()
    }
    pub fn link() -> Hsla {
        rgb(0x1d4f91).into()
    }
    /// Le fond arrondi sous le mot lu.
    pub fn word() -> Hsla {
        rgba(0xf2c94c99).into()
    }
    /// La phrase lue quand aucun mot ne l'est.
    pub fn sentence() -> Hsla {
        rgba(0xf2c94c40).into()
    }
    /// La règle sous la ligne lue.
    pub fn ruler() -> Hsla {
        rgba(0x8ab4f833).into()
    }
}

/// Marge autour du texte, en pixels.
const PAD: f32 = 24.0;
/// L'air autour du mot, dans son fond arrondi (`ReadingTextView.pillPadding`).
const PILL_PAD: (f32, f32) = (5.0, 4.0);
const PILL_RADIUS: f32 = 6.0;
/// La ligne lue se tient à 35 % de la hauteur de la vue
/// (`ReadingTextView.keepLineInPlace`) : dans le tiers haut.
const ANCHOR: f32 = 0.35;
/// Relecture de l'instantané pendant la lecture, et sinon.
const FAST: Duration = Duration::from_millis(16);
const SLOW: Duration = Duration::from_millis(250);
/// Au-delà, le cache des largeurs repart de zéro.
const CACHE_LIMIT: usize = 100_000;

/// La vue suit-elle la lecture ? Repris de `FollowPolicy.swift` : un
/// défilement à la main suspend le suivi ; il reprend à la prochaine
/// commande de lecture, ou quand la voix passe à une nouvelle phrase après
/// quatre secondes sans défilement.
#[derive(Debug, Default)]
struct FollowPolicy {
    suspended: bool,
    last_user_scroll: Option<Instant>,
}

impl FollowPolicy {
    const QUIET_DELAY: Duration = Duration::from_secs(4);

    fn user_scrolled(&mut self, now: Instant) {
        self.suspended = true;
        self.last_user_scroll = Some(now);
    }

    fn resume(&mut self) {
        self.suspended = false;
    }

    fn should_follow(&mut self, new_sentence: bool, now: Instant) -> bool {
        let quiet = self
            .last_user_scroll
            .is_none_or(|at| now.saturating_duration_since(at) >= Self::QUIET_DELAY);
        if self.suspended && new_sentence && quiet {
            self.suspended = false;
        }
        !self.suspended
    }
}

/// Ce qui distingue deux mesures de texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextStyle {
    bold: bool,
    italic: bool,
    mono: bool,
    /// Taille en centièmes de pixel.
    size: u32,
}

/// La police des réglages, et ce qui en découle pour chaque genre de bloc.
#[derive(Debug, Clone, PartialEq)]
struct Fonts {
    family: SharedString,
    size: f32,
    /// Hauteur de ligne en multiple de la taille, comme l'app Swift :
    /// 1,35 plus 0,25 par cran d'interligne.
    line_multiple: f32,
}

impl Fonts {
    fn of(settings: &SettingsView) -> Self {
        let size = if settings.reading_size.is_finite() && settings.reading_size > 0.0 {
            settings
                .reading_size
                .clamp(READING_SIZE_MIN, READING_SIZE_MAX)
        } else {
            READING_SIZE_DEFAULT
        };
        let family = match settings.reading_font.as_str() {
            "atkinson" => "Atkinson Hyperlegible",
            "openDyslexic" => "OpenDyslexic",
            _ => "Literata",
        };
        Fonts {
            family: family.into(),
            size,
            line_multiple: 1.35 + 0.25 * settings.line_spacing.min(3) as f32,
        }
    }

    fn mono_family() -> &'static str {
        if cfg!(target_os = "macos") {
            "Menlo"
        } else if cfg!(target_os = "windows") {
            "Consolas"
        } else {
            "DejaVu Sans Mono"
        }
    }

    fn size_of(&self, kind: &BlockKindView) -> f32 {
        match kind {
            BlockKindView::Heading { level: 1 } => self.size * 1.6,
            BlockKindView::Heading { level: 2 } => self.size * 1.35,
            BlockKindView::Heading { .. } => self.size * 1.15,
            _ => self.size,
        }
    }

    fn line_height(&self, kind: &BlockKindView) -> f32 {
        self.size_of(kind) * self.line_multiple
    }

    fn style(&self, word: Option<&WordView>, kind: &BlockKindView) -> TextStyle {
        let verbatim = matches!(
            kind,
            BlockKindView::Code { .. } | BlockKindView::TableRow { .. }
        );
        TextStyle {
            bold: matches!(kind, BlockKindView::Heading { .. })
                || matches!(kind, BlockKindView::TableRow { header: true })
                || word.is_some_and(|w| w.strong),
            italic: word.is_some_and(|w| w.italic),
            mono: verbatim || word.is_some_and(|w| w.code),
            size: (self.size_of(kind) * 100.0).round() as u32,
        }
    }

    fn font(&self, style: TextStyle) -> Font {
        let family: SharedString = if style.mono {
            Self::mono_family().into()
        } else {
            self.family.clone()
        };
        let mut f = font(family);
        if style.bold {
            f = f.bold();
        }
        if style.italic {
            f = f.italic();
        }
        f
    }

    fn run(&self, style: TextStyle, len: usize, word: Option<&WordView>) -> TextRun {
        let color = if word.is_some_and(|w| w.link) {
            colors::link()
        } else {
            colors::text()
        };
        TextRun {
            len,
            font: self.font(style),
            color,
            background_color: None,
            underline: None,
            strikethrough: word.filter(|w| w.struck).map(|_| StrikethroughStyle {
                thickness: px(1.0),
                color: Some(color),
            }),
        }
    }
}

type Widths = HashMap<TextStyle, HashMap<String, f32>>;

/// La mesure par le système de texte de GPUI, avec cache : la largeur d'un
/// texte dans un style (l'espace comprise) ne se demande qu'une fois.
struct GpuiMeasure<'a> {
    text: &'a WindowTextSystem,
    fonts: &'a Fonts,
    widths: &'a RefCell<Widths>,
}

impl Measure for GpuiMeasure<'_> {
    fn width(&self, text: &str, word: &WordView, kind: &BlockKindView) -> f32 {
        let style = self.fonts.style(Some(word), kind);
        if let Some(w) = self
            .widths
            .borrow()
            .get(&style)
            .and_then(|by_text| by_text.get(text))
        {
            return *w;
        }
        let run = self.fonts.run(style, text.len(), None);
        let width = self
            .text
            .layout_line(text, px(style.size as f32 / 100.0), &[run], None)
            .width
            .as_f32();
        self.widths
            .borrow_mut()
            .entry(style)
            .or_default()
            .insert(text.to_string(), width);
        width
    }

    fn line_height(&self, kind: &BlockKindView) -> f32 {
        self.fonts.line_height(kind)
    }
}

/// Ce dont dépend la mise en page.
#[derive(Debug, Clone, PartialEq)]
struct LayoutKey {
    document: usize,
    /// Largeur au demi-pixel.
    width: i64,
    fonts: Fonts,
}

/// Ce que la vue peint à une image : préparé en `prepaint`, peint ensuite.
struct Plan {
    hitbox: Hitbox,
    bounds: Bounds<Pixels>,
    quads: Vec<(Bounds<Pixels>, Hsla, f32)>,
    texts: Vec<Text>,
}

/// Un texte façonné, à peindre à `origin` sur une ligne de `height`,
/// rogné à `clip` s'il n'est qu'un morceau de mot.
struct Text {
    origin: Point<Pixels>,
    line: ShapedLine,
    height: Pixels,
    clip: Option<Bounds<Pixels>>,
}

pub struct ReadingView {
    session: Arc<Session>,
    snapshot: Snapshot,
    document: Arc<DocView>,
    fonts: Fonts,
    ruler: bool,
    widths: RefCell<Widths>,
    layout: Layout,
    layout_key: Option<LayoutKey>,
    layout_passes: usize,
    updates: usize,
    /// Le mot lu, en (id de phrase, position dans la phrase).
    highlight: Option<(usize, usize)>,
    /// Défilement, en pixels depuis le haut du texte.
    scroll: f32,
    /// Le cadre de la vue à la dernière image.
    bounds: Option<Bounds<Pixels>>,
    policy: FollowPolicy,
    /// Le haut de la ligne tenue en place la dernière fois.
    anchor_y: Option<f32>,
    _refresh: Task<()>,
}

impl ReadingView {
    pub fn new(session: Arc<Session>, cx: &mut Context<Self>) -> Self {
        let settings = session.settings().unwrap_or_default();
        let snapshot = session.snapshot();
        let document = session.document();
        let refresh = cx.spawn(async move |this, cx| loop {
            let Ok(playing) = this.update(cx, |view, cx| view.poll(cx)) else {
                break;
            };
            let wait = if playing { FAST } else { SLOW };
            cx.background_executor().timer(wait).await;
        });
        let mut view = ReadingView {
            session,
            snapshot,
            document,
            fonts: Fonts::of(&settings),
            ruler: settings.reading_ruler,
            widths: RefCell::default(),
            layout: Layout::default(),
            layout_key: None,
            layout_passes: 0,
            updates: 0,
            highlight: None,
            scroll: 0.0,
            bounds: None,
            policy: FollowPolicy::default(),
            anchor_y: None,
            _refresh: refresh,
        };
        view.highlight = view.locate();
        view
    }

    /// Vrai tant que la vue ramène la ligne lue.
    pub fn following(&self) -> bool {
        !self.policy.suspended
    }

    /// Reprend le suivi et recentre la ligne lue.
    pub fn follow(&mut self, cx: &mut Context<Self>) {
        self.policy.resume();
        self.anchor_y = None;
        self.place(true);
        cx.notify();
    }

    /// Va au premier mot de la ligne affichée voisine (seul le signe de
    /// `delta` compte). Sans mot lu, part de la première ligne de la phrase.
    pub fn line(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some((sentence, word)) = self.neighbour(delta) {
            self.seek(sentence, word, cx);
        }
    }

    /// Nouveaux réglages : police, taille, interligne, règle.
    pub fn set_settings(&mut self, settings: &SettingsView, cx: &mut Context<Self>) {
        self.fonts = Fonts::of(settings);
        self.ruler = settings.reading_ruler;
        cx.notify();
    }

    /// Le mot lu, en (id de phrase, position dans la phrase) ; `None` quand
    /// aucun mot ne l'est.
    pub fn highlighted(&self) -> Option<(usize, usize)> {
        self.highlight
    }

    /// La phrase lue.
    pub fn highlighted_sentence(&self) -> usize {
        self.snapshot.sentence
    }

    /// La mise en page de la dernière image.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Nombre de mises en page faites depuis la création.
    pub fn layout_passes(&self) -> usize {
        self.layout_passes
    }

    /// Nombre d'instantanés pris (et donc de redessins demandés) par
    /// `receive`.
    pub fn updates(&self) -> usize {
        self.updates
    }

    pub fn scroll_offset(&self) -> f32 {
        self.scroll
    }

    /// Le centre du mot, en coordonnées de fenêtre, à la dernière image.
    pub fn word_position(&self, sentence: usize, word: usize) -> Option<Point<Pixels>> {
        let bounds = self.bounds?;
        let (line, b) = self.layout.lines.iter().find_map(|line| {
            line.words
                .iter()
                .find(|b| b.sentence == sentence && b.word == word)
                .map(|b| (line, b))
        })?;
        Some(point(
            bounds.origin.x + px(PAD + b.x + b.width / 2.0),
            bounds.origin.y + px(PAD + line.y - self.scroll + line.height / 2.0),
        ))
    }

    /// Prend un instantané et le document de sa révision : c'est ce que la
    /// boucle de rafraîchissement appelle quand l'un des deux change.
    ///
    /// Un document qui prolonge le précédent (mêmes blocs, d'autres à la
    /// suite) garde le défilement et le suivi ; un autre document repart du
    /// haut, en suivant la lecture.
    pub fn receive(&mut self, snapshot: Snapshot, document: Arc<DocView>, cx: &mut Context<Self>) {
        if !Arc::ptr_eq(&document, &self.document) {
            if !continues(&self.document, &document) {
                self.scroll = 0.0;
                self.anchor_y = None;
                self.policy.resume();
            }
            self.document = document;
            self.layout_key = None;
        }
        let new_sentence = snapshot.sentence != self.snapshot.sentence;
        self.snapshot = snapshot;
        self.updates += 1;
        self.highlight = self.locate();
        let now = cx.background_executor().now();
        self.policy.should_follow(new_sentence, now);
        self.place(false);
        cx.notify();
    }

    /// Relit le lecteur ; rend vrai pendant la lecture.
    fn poll(&mut self, cx: &mut Context<Self>) -> bool {
        let snapshot = self.session.snapshot();
        if snapshot != self.snapshot {
            let document = if snapshot.revision != self.snapshot.revision {
                self.session.document()
            } else {
                Arc::clone(&self.document)
            };
            self.receive(snapshot, document, cx);
        }
        self.snapshot.playing
    }

    fn seek(&mut self, sentence: usize, word: usize, cx: &mut Context<Self>) {
        self.session
            .send(Request::Command(Command::SeekToWord { sentence, word }));
        // Une commande de lecture : la vue reprend le suivi.
        if self.policy.suspended {
            self.policy.resume();
            self.anchor_y = None;
        }
        cx.notify();
    }

    fn click_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds else { return };
        let x = (position.x - bounds.origin.x).as_f32() - PAD;
        let y = (position.y - bounds.origin.y).as_f32() - PAD + self.scroll;
        if let Some((sentence, word)) = self.layout.hit(x, y) {
            self.seek(sentence, word, cx);
        }
    }

    fn scroll_by(&mut self, delta: ScrollDelta, cx: &mut Context<Self>) {
        let line = px(self.fonts.line_height(&BlockKindView::Paragraph));
        let dy = delta.pixel_delta(line).y.as_f32();
        if dy == 0.0 || !dy.is_finite() {
            return;
        }
        self.scroll = (self.scroll - dy).clamp(0.0, self.max_scroll());
        self.policy.user_scrolled(cx.background_executor().now());
        self.anchor_y = None;
        cx.notify();
    }

    fn max_scroll(&self) -> f32 {
        let viewport = self.bounds.map_or(0.0, |b| b.size.height.as_f32());
        (self.layout.height + 2.0 * PAD - viewport).max(0.0)
    }

    /// Le mot lu, d'après l'instantané : `Snapshot.word` est l'index global
    /// du mot (`WordView.index`), ramené ici à sa position dans la phrase.
    fn locate(&self) -> Option<(usize, usize)> {
        let index = self.snapshot.word?;
        let sentence = self
            .document
            .blocks
            .iter()
            .flat_map(|b| &b.sentences)
            .find(|s| s.id == self.snapshot.sentence)?;
        let word = sentence.words.iter().position(|w| w.index == index)?;
        Some((sentence.id, word))
    }

    /// Le bloc qui porte une phrase.
    fn block_of(&self, sentence: usize) -> Option<usize> {
        self.document
            .blocks
            .iter()
            .position(|b| b.sentences.iter().any(|s| s.id == sentence))
    }

    /// La ligne qui tient la lecture : celle du mot lu, sinon la première
    /// de la phrase lue, sinon la première du bloc (code, tableau).
    fn anchor_line(&self) -> Option<usize> {
        let sentence = self.snapshot.sentence;
        if let Some((s, w)) = self.highlight {
            if let Some(i) = self.layout.line_of(s, w) {
                return Some(i);
            }
        }
        if let Some(i) = self
            .layout
            .lines
            .iter()
            .position(|l| l.words.iter().any(|b| b.sentence == sentence))
        {
            return Some(i);
        }
        let block = self.block_of(sentence)?;
        self.layout.lines.iter().position(|l| l.block == block)
    }

    fn neighbour(&self, delta: isize) -> Option<(usize, usize)> {
        if let Some((s, w)) = self.highlight {
            return self.layout.neighbour(s, w, delta);
        }
        // Sans mot lu : la ligne voisine de la première ligne de la phrase.
        let start = self.anchor_line()?;
        if let Some(first) = self.layout.lines[start].words.first() {
            return self.layout.neighbour(first.sentence, first.word, delta);
        }
        // La phrase n'a pas de mot affiché (code, tableau) : on enjambe ses
        // lignes jusqu'à une ligne qui en a.
        let step = delta.signum();
        if step == 0 {
            return None;
        }
        let mut i = start as isize;
        loop {
            i += step;
            let line = self.layout.lines.get(usize::try_from(i).ok()?)?;
            if let Some(first) = line.words.first() {
                return Some((first.sentence, first.word));
            }
        }
    }

    /// Tient la ligne lue à 35 % de la hauteur, si l'on suit la lecture et
    /// que cette ligne a changé (ou si `force`).
    fn place(&mut self, force: bool) {
        if self.policy.suspended || self.layout_key.is_none() {
            return;
        }
        let Some(bounds) = self.bounds else { return };
        let Some(line) = self.anchor_line().map(|i| &self.layout.lines[i]) else {
            return;
        };
        let (y, height) = (line.y, line.height);
        if !force && self.anchor_y == Some(y) {
            return;
        }
        self.anchor_y = Some(y);
        let wanted = PAD + y + height / 2.0 - bounds.size.height.as_f32() * ANCHOR;
        self.scroll = wanted.clamp(0.0, self.max_scroll());
    }

    /// Refait la mise en page si ce dont elle dépend a changé.
    fn ensure_layout(&mut self, width: f32, window: &Window) {
        let key = LayoutKey {
            document: Arc::as_ptr(&self.document) as usize,
            width: (width * 2.0).round() as i64,
            fonts: self.fonts.clone(),
        };
        if self.layout_key.as_ref() == Some(&key) {
            return;
        }
        let font_changed = self
            .layout_key
            .as_ref()
            .is_none_or(|old| old.fonts.family != key.fonts.family);
        if font_changed
            || self
                .widths
                .borrow()
                .values()
                .map(HashMap::len)
                .sum::<usize>()
                > CACHE_LIMIT
        {
            self.widths.borrow_mut().clear();
        }
        let text = window.text_system().clone();
        let measure = GpuiMeasure {
            text: &text,
            fonts: &self.fonts,
            widths: &self.widths,
        };
        // L'interligne est déjà dans la hauteur de ligne de la mesure.
        self.layout = lay_out(&self.document, width, 0, &measure);
        self.layout_passes += 1;
        self.layout_key = Some(key);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
    }

    /// Mise en page, suivi, puis tout ce qu'il faut peindre.
    fn prepare(&mut self, bounds: Bounds<Pixels>, hitbox: Hitbox, window: &Window) -> Plan {
        self.bounds = Some(bounds);
        let width = bounds.size.width.as_f32() - 2.0 * PAD;
        self.ensure_layout(width, window);
        self.place(false);

        let mut plan = Plan {
            hitbox,
            bounds,
            quads: Vec::new(),
            texts: Vec::new(),
        };
        let top = self.scroll - PAD;
        let bottom = self.scroll + bounds.size.height.as_f32();
        let origin_x = bounds.origin.x.as_f32() + PAD;
        let origin_y = bounds.origin.y.as_f32() + PAD - self.scroll;
        let first = self.layout.lines.partition_point(|l| l.y + l.height < top);
        let anchor = self.anchor_line();
        let text = window.text_system().clone();

        for i in first..self.layout.lines.len() {
            let line = &self.layout.lines[i];
            if line.y > bottom {
                break;
            }
            let Some(block) = self.document.blocks.get(line.block) else {
                continue;
            };
            let kind = &block.kind;
            let line_top = origin_y + line.y;
            let size = self.fonts.size_of(kind);
            let glyph = size * 1.3;
            let pill_top = line_top + (line.height - glyph) / 2.0 - PILL_PAD.1;
            let pill_height = glyph + 2.0 * PILL_PAD.1;

            if self.ruler && anchor == Some(i) {
                plan.quads.push((
                    rect(
                        bounds.origin.x.as_f32(),
                        line_top - 2.0,
                        bounds.size.width.as_f32(),
                        line.height + 4.0,
                    ),
                    colors::ruler(),
                    0.0,
                ));
            }

            if line.words.is_empty() {
                // Code, tableau : la ligne telle quelle, sans mots suivis.
                // Son rang parmi les lignes du bloc.
                let k = self.layout.lines[..i]
                    .iter()
                    .rev()
                    .take_while(|l| l.block == line.block)
                    .count();
                let shown: String = block
                    .display_lines
                    .get(k)
                    .map(|s| s.replace(['\n', '\r'], " "))
                    .unwrap_or_default();
                if self.highlight.is_none()
                    && self.block_of(self.snapshot.sentence) == Some(line.block)
                {
                    plan.quads.push((
                        rect(
                            origin_x - PILL_PAD.0,
                            pill_top,
                            width + 2.0 * PILL_PAD.0,
                            pill_height,
                        ),
                        colors::sentence(),
                        PILL_RADIUS,
                    ));
                }
                let style = self.fonts.style(None, kind);
                let run = self.fonts.run(style, shown.len(), None);
                let shaped = text.shape_line(shown.into(), px(size), &[run], None);
                plan.texts.push(Text {
                    origin: point(px(origin_x), px(line_top)),
                    line: shaped,
                    height: px(line.height),
                    clip: None,
                });
                continue;
            }

            for (j, b) in line.words.iter().enumerate() {
                let Some(word) = block
                    .sentences
                    .iter()
                    .find(|s| s.id == b.sentence)
                    .and_then(|s| s.words.get(b.word))
                else {
                    continue;
                };
                let lit = if self.highlight == Some((b.sentence, b.word)) {
                    Some(colors::word())
                } else if self.highlight.is_none() && b.sentence == self.snapshot.sentence {
                    Some(colors::sentence())
                } else {
                    None
                };
                if let Some(color) = lit {
                    plan.quads.push((
                        rect(
                            origin_x + b.x - PILL_PAD.0,
                            pill_top,
                            b.width + 2.0 * PILL_PAD.0,
                            pill_height,
                        ),
                        color,
                        PILL_RADIUS,
                    ));
                }
                // Un mot coupé sur plusieurs lignes : chaque boîte montre sa
                // part du mot entier, décalé de ce que montrent les boîtes
                // d'avant.
                let offset = if j == 0 { self.split_offset(i, b) } else { 0.0 };
                let style = self.fonts.style(Some(word), kind);
                let run = self.fonts.run(style, word.text.len(), Some(word));
                let shaped = text.shape_line(word.text.clone().into(), px(size), &[run], None);
                let clip = (offset > 0.0 || self.is_split(i, b))
                    .then(|| rect(origin_x + b.x, line_top, b.width, line.height));
                plan.texts.push(Text {
                    origin: point(px(origin_x + b.x - offset), px(line_top)),
                    line: shaped,
                    height: px(line.height),
                    clip,
                });
            }
        }
        plan
    }

    /// La largeur du mot déjà montrée par les lignes d'avant, quand la
    /// boîte `b`, en tête de la ligne `i`, en continue un coupé.
    fn split_offset(&self, i: usize, b: &WordBox) -> f32 {
        self.layout.lines[..i]
            .iter()
            .rev()
            .take_while(|l| same_word_alone(l, b))
            .map(|l| l.words[0].width)
            .sum()
    }

    /// Vrai si la boîte `b` de la ligne `i` est un morceau de mot.
    fn is_split(&self, i: usize, b: &WordBox) -> bool {
        let line = &self.layout.lines[i];
        let alone_first = line.words.first() == Some(b);
        let alone_last = line.words.last() == Some(b);
        (alone_first && i > 0 && same_word_alone(&self.layout.lines[i - 1], b))
            || (alone_last
                && line.words.len() == 1
                && self
                    .layout
                    .lines
                    .get(i + 1)
                    .and_then(|l| l.words.first())
                    .is_some_and(|n| (n.sentence, n.word) == (b.sentence, b.word)))
    }
}

/// La ligne ne montre qu'un morceau du même mot que `b`.
fn same_word_alone(line: &Line, b: &WordBox) -> bool {
    line.words.len() == 1 && (line.words[0].sentence, line.words[0].word) == (b.sentence, b.word)
}

/// `new` prolonge `old` : mêmes blocs, sauf le dernier qui a pu grandir, et
/// d'autres éventuellement à la suite.
fn continues(old: &DocView, new: &DocView) -> bool {
    let Some((last, before)) = old.blocks.split_last() else {
        return false;
    };
    new.blocks.len() >= old.blocks.len()
        && new.blocks[..before.len()] == *before
        && new.blocks[before.len()].id == last.id
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(x), px(y)),
        size: size(px(width.max(0.0)), px(height.max(0.0))),
    }
}

impl Render for ReadingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(colors::background())
            .child(ReadingElement { view: cx.entity() })
    }
}

/// L'élément qui peint le texte, le mot lu et la règle, et reçoit clic et
/// molette.
struct ReadingElement {
    view: Entity<ReadingView>,
}

impl Element for ReadingElement {
    type RequestLayoutState = ();
    type PrepaintState = Plan;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Plan {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.view
            .update(cx, |view, _cx| view.prepare(bounds, hitbox, window))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        plan: &mut Plan,
        window: &mut Window,
        cx: &mut App,
    ) {
        let bounds = plan.bounds;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (quad, color, radius) in &plan.quads {
                window.paint_quad(fill(*quad, *color).corner_radii(px(*radius)));
            }
            for text in &plan.texts {
                // Un mot coupé se rogne à sa boîte ; le reste, au cadre.
                let clip = text.clip.map(|bounds| ContentMask { bounds });
                window.with_content_mask(clip, |window| {
                    let _ = text.line.paint(
                        text.origin,
                        text.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                });
            }
        });

        let hitbox = plan.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
            {
                view.update(cx, |view, cx| view.click_at(event.position, cx));
            }
        });
        let hitbox = plan.hitbox.clone();
        let view = self.view.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                view.update(cx, |view, cx| view.scroll_by(event.delta, cx));
            }
        });
    }
}

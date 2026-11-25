//! Le thème de l'app : les deux palettes (Papier, Nuit), le choix des
//! réglages (`config::Theme`) et l'apparence du système, en un seul endroit.
//!
//! L'identité vient du lecteur lui-même : un papier chaud, jamais blanc pur
//! (moins d'éblouissement pour un lecteur dyslexique), une encre presque
//! noire, jamais noire pure ; une seule couleur d'accent, l'ambre du mot
//! lu, que reprennent le disque de lecture, la part lue de la frise et le
//! bouton « Écouter ». Le bleu ne sert qu'au focus du clavier et aux liens.
//! La nuit garde la même chaleur : un brun très sombre, un texte crème.
//!
//! Chaque couleur des composants de gpui-component (survol, sélection,
//! focus, bouton principal…) est tirée de la palette (`component_colors`) :
//! rien du thème par défaut ne reste. Les contrastes sont mesurés (WCAG 2,
//! `contrast`) par `palette_meets_contrast`.

use dysflow::config::Theme;
use gpui::{rgb, App, Global, Hsla, Rgba, WindowAppearance};
use gpui_component::theme::{Theme as ComponentTheme, ThemeColor, ThemeMode};

/// Une couleur opaque, en `0xRRGGBB` (sRGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u32);

impl Rgb {
    fn channels(self) -> [f32; 3] {
        [16, 8, 0].map(|shift| ((self.0 >> shift) & 0xff) as f32 / 255.0)
    }

    /// La luminance relative de WCAG 2 (sRGB linéarisé).
    pub fn luminance(self) -> f32 {
        let [r, g, b] = self.channels().map(|c| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    pub fn hsla(self) -> Hsla {
        rgb(self.0).into()
    }
}

/// Le rapport de contraste WCAG 2 entre deux couleurs, de 1 à 21.
pub fn contrast(fg: Rgb, bg: Rgb) -> f32 {
    let (a, b) = (fg.luminance(), bg.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Une teinte translucide, posée sur un fond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    pub color: Rgb,
    pub alpha: f32,
}

const fn tint(color: u32, alpha: f32) -> Tint {
    Tint {
        color: Rgb(color),
        alpha,
    }
}

impl Tint {
    pub fn hsla(self) -> Hsla {
        Rgba {
            a: self.alpha,
            ..rgb(self.color.0)
        }
        .into()
    }

    /// La couleur vue : la teinte posée sur `bg`, au canal près.
    pub fn over(self, bg: Rgb) -> Rgb {
        let [fr, fg, fb] = self.color.channels();
        let [br, bgc, bb] = bg.channels();
        let mix =
            |f: f32, b: f32| ((f * self.alpha + b * (1.0 - self.alpha)) * 255.0).round() as u32;
        Rgb((mix(fr, br) << 16) | (mix(fg, bgc) << 8) | mix(fb, bb))
    }
}

/// Clair ou sombre : ce que la fenêtre montre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// Le thème choisi dans les réglages, l'apparence du système à côté :
/// « Système » la suit, « Clair » et « Sombre » s'y tiennent.
pub fn resolve(theme: Theme, system_dark: bool) -> Appearance {
    match theme {
        Theme::Light => Appearance::Light,
        Theme::Dark => Appearance::Dark,
        Theme::System if system_dark => Appearance::Dark,
        Theme::System => Appearance::Light,
    }
}

/// L'apparence de GPUI est-elle sombre ?
pub fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Les couleurs d'une apparence.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub name: &'static str,
    /// La page : la lecture, l'éditeur, l'aide.
    pub background: Rgb,
    /// Ce qui est posé sur la page : barre de lecture, sommaire, réglages.
    pub surface: Rgb,
    /// Les cartes des réglages, posées sur la surface.
    pub card: Rgb,
    /// Le texte lu et les titres.
    pub text: Rgb,
    /// Les libellés de l'interface.
    pub chrome: Rgb,
    /// Le texte secondaire : temps, descriptions, barre d'état.
    pub muted: Rgb,
    /// Un titre sans rien à lire (sommaire) : en retrait, mais lisible.
    pub disabled: Rgb,
    pub link: Rgb,
    pub error: Rgb,
    /// Les filets entre deux zones (décor seulement).
    pub border: Rgb,
    /// Le bord d'un champ : on voit où écrire (3:1 au moins).
    pub control: Rgb,
    /// Le focus du clavier et la sélection au clavier.
    pub focus: Rgb,
    /// L'ambre : disque de lecture, part lue, « Écouter », section lue.
    pub accent: Rgb,
    /// Ce qui s'écrit sur l'accent.
    pub on_accent: Rgb,
    /// Le fond arrondi sous le mot lu.
    pub word: Tint,
    /// La phrase lue quand aucun mot ne l'est.
    pub sentence: Tint,
    /// La règle sous la ligne lue.
    pub ruler: Tint,
    pub hover: Tint,
    pub selected: Tint,
    /// Le texte sélectionné dans un champ.
    pub selection: Tint,
    /// Le voile sous l'aide.
    pub overlay: Tint,
    /// Le fond d'une touche dans l'aide et l'éditeur.
    pub keycap: Tint,
    /// Le fond du texte en cours de modification.
    pub editing: Tint,
    /// La page sous un fichier qu'on dépose.
    pub drop_target: Tint,
    /// Le pouce des barres de défilement.
    pub scroll_thumb: Tint,
}

/// Papier : le jour.
pub const LIGHT: Palette = Palette {
    name: "Papier",
    background: Rgb(0xfbf8f1),
    surface: Rgb(0xf2ede2),
    card: Rgb(0xfffdf8),
    text: Rgb(0x1f1d1a),
    chrome: Rgb(0x45413a),
    muted: Rgb(0x5c564c),
    disabled: Rgb(0x645e54),
    link: Rgb(0x1d4f91),
    error: Rgb(0x9e3220),
    border: Rgb(0xe4ddcf),
    control: Rgb(0x857e71),
    focus: Rgb(0x2b5797),
    accent: Rgb(0xb7791f),
    on_accent: Rgb(0x1f1d1a),
    word: tint(0xf2c94c, 0.6),
    sentence: tint(0xf2c94c, 0.25),
    ruler: tint(0x8ab4f8, 0.2),
    hover: tint(0x1f1d1a, 0.06),
    selected: tint(0x1f1d1a, 0.10),
    selection: tint(0x2b5797, 0.25),
    overlay: tint(0x1f1d1a, 0.2),
    keycap: tint(0x1f1d1a, 0.07),
    editing: tint(0x1f1d1a, 0.05),
    drop_target: tint(0xb7791f, 0.12),
    scroll_thumb: tint(0x5c564c, 0.45),
};

/// Nuit : le soir.
pub const DARK: Palette = Palette {
    name: "Nuit",
    background: Rgb(0x1a1815),
    surface: Rgb(0x24211d),
    card: Rgb(0x28251f),
    text: Rgb(0xece6da),
    chrome: Rgb(0xd2c9b9),
    muted: Rgb(0xb5ac9c),
    disabled: Rgb(0xaba292),
    link: Rgb(0x9dc1fa),
    error: Rgb(0xf59478),
    border: Rgb(0x37332d),
    control: Rgb(0x857c6d),
    focus: Rgb(0x8ab4f8),
    accent: Rgb(0xe5a93f),
    on_accent: Rgb(0x1a1815),
    word: tint(0xe5a93f, 0.28),
    sentence: tint(0xe5a93f, 0.14),
    ruler: tint(0x8ab4f8, 0.12),
    hover: tint(0xece6da, 0.06),
    selected: tint(0xece6da, 0.10),
    selection: tint(0x8ab4f8, 0.3),
    overlay: tint(0x000000, 0.5),
    keycap: tint(0xece6da, 0.08),
    editing: tint(0xece6da, 0.04),
    drop_target: tint(0xe5a93f, 0.14),
    scroll_thumb: tint(0xb5ac9c, 0.45),
};

pub fn palette(appearance: Appearance) -> &'static Palette {
    match appearance {
        Appearance::Light => &LIGHT,
        Appearance::Dark => &DARK,
    }
}

/// Les couleurs prêtes à peindre (`Hsla`), du même nom que les champs.
macro_rules! paint {
    ($($field:ident),* $(,)?) => {
        impl Palette {
            $(pub fn $field(&self) -> Hsla { self.$field.hsla() })*
        }
    };
}

paint!(
    background,
    surface,
    card,
    text,
    chrome,
    muted,
    disabled,
    link,
    error,
    border,
    control,
    focus,
    accent,
    on_accent,
    word,
    sentence,
    ruler,
    hover,
    selected,
    selection,
    overlay,
    keycap,
    editing,
    drop_target,
    scroll_thumb,
);

impl Palette {
    /// Les fonds sur lesquels du texte se pose, survol et sélection
    /// compris (ce que l'œil voit, la teinte posée).
    fn grounds(&self) -> Vec<(Rgb, String)> {
        let mut grounds = Vec::new();
        for (bg, name) in [
            (self.background, "page"),
            (self.surface, "surface"),
            (self.card, "carte"),
        ] {
            grounds.push((bg, name.to_string()));
            for (t, state) in [(self.hover, "survol"), (self.selected, "sélection")] {
                grounds.push((t.over(bg), format!("{state} sur {name}")));
            }
        }
        grounds
    }

    /// Chaque paire texte/fond de l'app : 4,5:1 au moins.
    pub fn text_pairs(&self) -> Vec<(Rgb, Rgb, String)> {
        let mut pairs = Vec::new();
        for (fg, name) in [
            (self.text, "texte"),
            (self.chrome, "libellé"),
            (self.muted, "secondaire"),
            (self.disabled, "désactivé"),
            (self.link, "lien"),
            (self.error, "erreur"),
        ] {
            for (bg, ground) in self.grounds() {
                pairs.push((fg, bg, format!("{name} sur {ground}")));
            }
        }
        let page = self.background;
        // Le mot lu, la phrase lue, la règle, et le mot lu sur la règle.
        for (t, name) in [
            (self.word, "mot lu"),
            (self.sentence, "phrase lue"),
            (self.ruler, "règle"),
            (self.editing, "édition"),
            (self.selection, "texte sélectionné"),
        ] {
            pairs.push((self.text, t.over(page), format!("texte sur {name}")));
        }
        let on_ruler = self.word.over(self.ruler.over(page));
        pairs.push((self.text, on_ruler, "mot lu sur la règle".into()));
        pairs.push((self.link, self.word.over(page), "lien sur le mot lu".into()));
        for bg in [self.surface, page] {
            pairs.push((self.text, self.keycap.over(bg), "touche".into()));
            pairs.push((
                self.chrome,
                self.keycap.over(bg),
                "libellé de touche".into(),
            ));
        }
        // Les pleins : l'accent, le bouton principal à l'encre, l'erreur.
        pairs.push((self.on_accent, self.accent, "sur l'accent".into()));
        pairs.push((self.background, self.text, "bouton principal".into()));
        pairs.push((
            self.background,
            self.chrome,
            "bouton principal survolé".into(),
        ));
        pairs.push((self.background, self.error, "bouton d'alerte".into()));
        pairs.push((self.background, self.focus, "bouton d'information".into()));
        pairs
    }

    /// Les indicateurs (focus, sélection au clavier, accent, bord des
    /// champs) contre leur fond : 3:1 au moins.
    pub fn indicator_pairs(&self) -> Vec<(Rgb, Rgb, String)> {
        let mut pairs = Vec::new();
        for (bg, ground) in self.grounds() {
            pairs.push((self.focus, bg, format!("focus sur {ground}")));
        }
        for (bg, ground) in [
            (self.background, "page"),
            (self.surface, "surface"),
            (self.card, "carte"),
        ] {
            pairs.push((self.accent, bg, format!("accent sur {ground}")));
            pairs.push((self.control, bg, format!("bord de champ sur {ground}")));
            pairs.push((self.text, bg, format!("pouce de curseur sur {ground}")));
        }
        pairs
    }
}

/// L'état du thème : le choix des réglages et l'apparence du système.
#[derive(Debug, Clone, Copy)]
pub struct ThemeState {
    pub choice: Theme,
    pub system_dark: bool,
}

impl Global for ThemeState {}

/// La palette en place ; sans état installé, celle du jour.
pub fn colors(cx: &App) -> &'static Palette {
    palette(appearance(cx))
}

pub fn appearance(cx: &App) -> Appearance {
    cx.try_global::<ThemeState>()
        .map_or(Appearance::Light, |s| resolve(s.choice, s.system_dark))
}

/// Pose le thème au démarrage : le choix lu dans `config.toml`, l'apparence
/// du système telle que GPUI la voit.
pub fn install(choice: Theme, cx: &mut App) {
    let system_dark = is_dark(cx.window_appearance());
    cx.set_global(ThemeState {
        choice,
        system_dark,
    });
    apply(cx);
}

/// Le choix des réglages a changé. Revenir à « Système » relit l'apparence
/// du système : elle a pu changer pendant qu'un thème était imposé.
pub fn set_choice(choice: Theme, cx: &mut App) {
    let mut state = current(cx);
    if state.choice == choice && cx.has_global::<ThemeState>() {
        return;
    }
    state.choice = choice;
    if choice == Theme::System {
        // Le cadre des fenêtres suit de nouveau le système (voir `apply`),
        // puis on lit ce qu'il montre.
        cx.set_window_appearance(None);
        state.system_dark = is_dark(cx.window_appearance());
    }
    cx.set_global(state);
    apply(cx);
}

/// L'apparence du système a changé (la fenêtre le dit). Sous un thème
/// imposé, la fenêtre ne fait que renvoyer celui-ci : rien ne change.
pub fn set_system_appearance(appearance: WindowAppearance, cx: &mut App) {
    let mut state = current(cx);
    if state.choice != Theme::System && cx.has_global::<ThemeState>() {
        return;
    }
    let dark = is_dark(appearance);
    if state.system_dark == dark && cx.has_global::<ThemeState>() {
        return;
    }
    state.system_dark = dark;
    cx.set_global(state);
    apply(cx);
}

/// Pose le thème s'il ne l'est pas encore (une fenêtre ouverte sans
/// `install`, comme dans les essais).
pub fn ensure(cx: &mut App) {
    if !cx.has_global::<ThemeState>() {
        install(Theme::System, cx);
    }
}

fn current(cx: &App) -> ThemeState {
    cx.try_global::<ThemeState>()
        .copied()
        .unwrap_or(ThemeState {
            choice: Theme::System,
            system_dark: false,
        })
}

/// La police de l'interface : Atkinson Hyperlegible, embarquée
/// (`fonts::EMBEDDED`), dessinée pour être lue sans confondre les lettres.
pub const UI_FONT: &str = "Atkinson Hyperlegible";

/// Applique l'apparence voulue à gpui-component (le mode, puis toutes ses
/// couleurs) et redessine chaque fenêtre.
fn apply(cx: &mut App) {
    let appearance = appearance(cx);
    let p = palette(appearance);
    let mode = match appearance {
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
    };
    // Le mode charge le thème inscrit de gpui-component : nos couleurs
    // passent ensuite, dans une seconde modification (voir `Theme::update`).
    ComponentTheme::change(mode, None, cx);
    ComponentTheme::update(cx, |theme| {
        theme.colors = component_colors(p);
        theme.font_family = UI_FONT.into();
    });
    // Le cadre des fenêtres (barre de titre du Mac) suit un thème imposé ;
    // « Système » le laisse au système. Sans effet hors du Mac.
    let state = current(cx);
    cx.set_window_appearance(match state.choice {
        Theme::System => None,
        Theme::Light => Some(WindowAppearance::Light),
        Theme::Dark => Some(WindowAppearance::Dark),
    });
    cx.refresh_windows();
}

/// Toutes les couleurs de gpui-component, tirées de la palette.
pub fn component_colors(p: &Palette) -> ThemeColor {
    paint_over(ThemeColor::default(), p)
}

/// Repeint chaque champ de `c` : aucun ne doit garder la valeur de départ
/// (`every_component_color_is_assigned`).
fn paint_over(mut c: ThemeColor, p: &Palette) -> ThemeColor {
    let transparent = Tint {
        color: p.background,
        alpha: 0.0,
    }
    .hsla();
    let light = |color: Rgb| Tint { color, alpha: 0.15 }.hsla();

    // Le fond, le texte, les filets.
    c.background = p.background();
    c.foreground = p.text();
    c.border = p.border();
    c.window_border = p.border();
    c.caret = p.text();
    c.ring = p.focus();
    c.selection = p.selection();
    c.overlay = p.overlay();
    c.input = p.control();
    c.drag_border = p.focus();
    c.drop_target = p.drop_target();
    c.link = p.link();
    c.link_hover = p.link();
    c.link_active = p.link();
    c.skeleton = p.hover();

    // Survol (menus, listes, boutons fantômes) et choix.
    c.accent = p.hover();
    c.accent_foreground = p.text();
    c.muted = p.surface();
    c.muted_foreground = p.muted();
    c.secondary = p.surface();
    c.secondary_hover = p.hover();
    c.secondary_active = p.selected();
    c.secondary_foreground = p.chrome();

    // L'action principale, à l'encre.
    c.primary = p.text();
    c.primary_hover = p.chrome();
    c.primary_active = p.text();
    c.primary_foreground = p.background();

    // Les boutons.
    c.button = p.card();
    c.button_hover = p.hover();
    c.button_active = p.selected();
    c.button_foreground = p.text();
    c.button_primary = p.text();
    c.button_primary_hover = p.chrome();
    c.button_primary_active = p.text();
    c.button_primary_foreground = p.background();
    c.button_secondary = p.surface();
    c.button_secondary_hover = p.hover();
    c.button_secondary_active = p.selected();
    c.button_secondary_foreground = p.text();
    c.button_danger = p.error();
    c.button_danger_hover = p.error();
    c.button_danger_active = p.error();
    c.button_danger_foreground = p.background();
    c.button_info = p.focus();
    c.button_info_hover = p.focus();
    c.button_info_active = p.focus();
    c.button_info_foreground = p.background();
    c.button_success = p.accent();
    c.button_success_hover = p.accent();
    c.button_success_active = p.accent();
    c.button_success_foreground = p.on_accent();
    c.button_warning = p.accent();
    c.button_warning_hover = p.accent();
    c.button_warning_active = p.accent();
    c.button_warning_foreground = p.on_accent();

    // Les états.
    c.danger = p.error();
    c.danger_hover = p.error();
    c.danger_active = p.error();
    c.danger_foreground = p.background();
    c.info = p.focus();
    c.info_hover = p.focus();
    c.info_active = p.focus();
    c.info_foreground = p.background();
    c.success = p.accent();
    c.success_hover = p.accent();
    c.success_active = p.accent();
    c.success_foreground = p.on_accent();
    c.warning = p.accent();
    c.warning_hover = p.accent();
    c.warning_active = p.accent();
    c.warning_foreground = p.on_accent();

    // Les listes (sommaire) et les tableaux.
    c.list = p.surface();
    c.list_even = p.surface();
    c.list_head = p.surface();
    c.list_hover = p.hover();
    c.list_active = p.selected();
    c.list_active_border = p.focus();
    c.list_highlight_foreground = p.text();
    c.table = p.card();
    c.table_even = p.surface();
    c.table_head = p.surface();
    c.table_head_foreground = p.chrome();
    c.table_foot = p.surface();
    c.table_foot_foreground = p.chrome();
    c.table_hover = p.hover();
    c.table_active = p.selected();
    c.table_active_border = p.focus();
    c.table_row_border = p.border();

    // Les surfaces posées.
    c.popover = p.card();
    c.popover_foreground = p.text();
    c.accordion = p.card();
    c.group_box = p.card();
    c.group_box_foreground = p.text();
    c.description_list_label = p.surface();
    c.description_list_label_foreground = p.chrome();
    c.sidebar = p.surface();
    c.sidebar_foreground = p.chrome();
    c.sidebar_border = p.border();
    c.sidebar_accent = p.selected();
    c.sidebar_accent_foreground = p.text();
    c.sidebar_primary = p.text();
    c.sidebar_primary_foreground = p.background();
    c.title_bar = p.background();
    c.title_bar_border = p.border();
    c.status_bar = p.background();
    c.status_bar_border = p.border();
    c.tab = transparent;
    c.tab_bar = p.surface();
    c.tab_bar_segmented = p.surface();
    c.tab_active = p.card();
    c.tab_active_foreground = p.text();
    c.tab_foreground = p.chrome();

    // Les curseurs, la progression, l'interrupteur, le défilement.
    c.slider_bar = p.accent();
    c.slider_thumb = p.text();
    c.progress_bar = p.accent();
    c.switch = p.control();
    c.switch_thumb = p.card();
    c.scrollbar = transparent;
    c.scrollbar_thumb = p.scroll_thumb();
    c.scrollbar_thumb_hover = p.muted();

    // Les graphiques et les teintes nommées : la palette, sans autre teinte.
    c.chart_1 = p.accent();
    c.chart_2 = p.focus();
    c.chart_3 = p.link();
    c.chart_4 = p.error();
    c.chart_5 = p.muted();
    c.chart_bullish = p.accent();
    c.chart_bearish = p.error();
    c.chart_grid = p.border();
    c.red = p.error();
    c.red_light = light(p.error);
    c.green = p.accent();
    c.green_light = light(p.accent);
    c.blue = p.focus();
    c.blue_light = light(p.focus);
    c.yellow = p.accent();
    c.yellow_light = light(p.accent);
    c.magenta = p.link();
    c.magenta_light = light(p.link);
    c.cyan = p.focus();
    c.cyan_light = light(p.focus);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_is_wcag_2() {
        assert!((contrast(Rgb(0x000000), Rgb(0xffffff)) - 21.0).abs() < 1e-3);
        assert!((contrast(Rgb(0xffffff), Rgb(0x000000)) - 21.0).abs() < 1e-3);
        assert!((contrast(Rgb(0x777777), Rgb(0x777777)) - 1.0).abs() < 1e-6);
        // Valeurs de référence (WebAIM) : #767676 sur blanc, 4,54:1.
        assert!((contrast(Rgb(0x767676), Rgb(0xffffff)) - 4.54).abs() < 0.01);
    }

    #[test]
    fn resolve_follows_the_system_only_for_system() {
        assert_eq!(resolve(Theme::System, false), Appearance::Light);
        assert_eq!(resolve(Theme::System, true), Appearance::Dark);
        assert_eq!(resolve(Theme::Light, true), Appearance::Light);
        assert_eq!(resolve(Theme::Dark, false), Appearance::Dark);
    }

    #[test]
    fn palette_meets_contrast() {
        for p in [&LIGHT, &DARK] {
            for (fg, bg, what) in p.text_pairs() {
                let ratio = contrast(fg, bg);
                // `--nocapture` : la table des rapports, pour le rapport.
                println!("{} | texte | {what} | {ratio:.2}", p.name);
                assert!(ratio >= 4.5, "{} {what} : {ratio:.2}:1", p.name);
            }
            for (fg, bg, what) in p.indicator_pairs() {
                let ratio = contrast(fg, bg);
                println!("{} | repère | {what} | {ratio:.2}", p.name);
                assert!(ratio >= 3.0, "{} {what} : {ratio:.2}:1", p.name);
            }
        }
    }

    #[test]
    fn every_component_color_is_assigned() {
        // Trois départs différents : un champ oublié garderait la valeur de
        // son départ, et les trois résultats différeraient sur ce champ.
        let bases = [
            ThemeColor::default(),
            *ThemeColor::light(),
            *ThemeColor::dark(),
        ];
        let starts: Vec<_> = bases.iter().map(|b| fields(&format!("{b:?}"))).collect();
        assert!(starts[0].len() > 100, "toutes les couleurs sont lues");
        for p in [&LIGHT, &DARK] {
            let painted: Vec<_> = bases
                .iter()
                .map(|b| fields(&format!("{:?}", paint_over(*b, p))))
                .collect();
            for field in starts[0].keys() {
                let same_result = painted[1][field] == painted[0][field]
                    && painted[2][field] == painted[0][field];
                // Un champ égal dans les trois départs (transparent partout,
                // comme `tab`) : il doit au moins avoir changé.
                let blind =
                    starts[1][field] == starts[0][field] && starts[2][field] == starts[0][field];
                assert!(
                    same_result && (!blind || painted[0][field] != starts[0][field]),
                    "{} : {field} n'est pas repeint",
                    p.name
                );
            }
        }
    }

    /// `champ: Hsla { … }` de l'affichage de débogage de `ThemeColor`.
    fn fields(debug: &str) -> std::collections::BTreeMap<String, String> {
        let body = debug
            .trim_start_matches("ThemeColor {")
            .trim_end_matches('}');
        body.split("}, ")
            .filter_map(|part| {
                let (name, value) = part.split_once(": ")?;
                Some((
                    name.trim().to_string(),
                    format!("{} }}", value.trim().trim_end_matches(['}', ' '])),
                ))
            })
            .collect()
    }
}

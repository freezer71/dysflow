//! La barre de lecture, reprise de la barre du lecteur de l'app Mac
//! (`ReaderBar`, `MiniPlayerView`, spec des modes d'affichage) :
//!
//! ```text
//! ⏮ (▶) ⏭   0:56 ━━━━━━━━━──────── -2:00   1,1×   ⚙
//! ```
//!
//! Le transport à gauche (phrase précédente, lecture/pause en disque
//! d'accent, seule touche de couleur de la barre, phrase suivante), puis le
//! temps écoulé, la frise cliquable, le temps restant estimé, la vitesse
//! (un menu des vitesses usuelles) et l'écrou des réglages. Cibles de 32 px
//! au moins, pictogrammes de 16 px, disque de 36 px (pictogramme de 18 px) ;
//! chiffres à chasse fixe (`tnum` d'Atkinson Hyperlegible). Sans document, tout est désactivé (pas caché).
//!
//! Aucun contrôle ne garde le focus : un clic laisse les touches à la vue
//! de lecture (sinon Espace rappuierait le bouton au lieu de lire).

use std::sync::Arc;

use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::view::Snapshot;
use gpui::{
    div, prelude::*, px, Context, Entity, EventEmitter, FocusHandle, FontWeight, IntoElement,
    SharedString, Subscription, Window,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::{Disableable, Selectable, Sizable};
use gpui_kit_assets::IconName;

use crate::session::Session;
use crate::theme;

/// Les vitesses du menu, comme l'app Mac.
pub const SPEEDS: [f32; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];

/// La cible de chaque bouton de barre.
const TARGET: f32 = 32.0;
/// Le disque de lecture.
const DISC: f32 = 36.0;

/// Le temps, à la seconde : « 2:05 », « –:–– » tant qu'il n'est pas connu.
pub fn time(seconds: Option<f64>) -> String {
    match seconds {
        Some(s) if s.is_finite() => {
            let t = s.max(0.0).round() as u64;
            format!("{}:{:02}", t / 60, t % 60)
        }
        _ => "–:––".to_string(),
    }
}

/// Le temps restant estimé : « -2:00 », « –:–– » tant que la durée n'est
/// pas connue.
pub fn remaining(elapsed: Option<f64>, total: Option<f64>) -> String {
    match (elapsed, total) {
        (Some(e), Some(t)) if e.is_finite() && t.is_finite() => {
            format!("-{}", time(Some((t - e).max(0.0))))
        }
        _ => time(None),
    }
}

/// La vitesse : deux décimales au plus, sans zéro inutile, avec la virgule
/// en français (« 1× », « 1,25× », « 1,1× »).
pub fn speed(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let mut text = format!("{rounded:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if dysflow::i18n::ui_lang() == dysflow::i18n::UiLang::Fr {
        text = text.replace('.', ",");
    }
    format!("{text}×")
}

/// Lecture possible : dès qu'un document est ouvert, quelle que soit la
/// voix. Celle de repli (Kokoro pas encore installé, ou en échec) lit tout
/// de suite : on n'attend pas la fin d'un téléchargement pour lire.
pub fn can_play(s: &Snapshot) -> bool {
    s.has_document
}

/// La phrase sous une part de la frise (0 à 1) ; `None` sans phrase.
pub fn sentence_at(fraction: f32, sentence_count: usize) -> Option<usize> {
    if sentence_count == 0 || !fraction.is_finite() {
        return None;
    }
    let f = fraction.clamp(0.0, 1.0);
    Some(((f * sentence_count as f32) as usize).min(sentence_count - 1))
}

/// Ce que la barre demande à la fenêtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEvent {
    /// L'écrou : ouvrir (ou fermer) les réglages.
    Settings,
}

pub struct ReaderBar {
    session: Arc<Session>,
    /// Où rendre le focus après un geste sur la frise ou le menu.
    focus: FocusHandle,
    snapshot: Snapshot,
    sentence_count: usize,
    timeline: Entity<SliderState>,
    /// Vrai pendant qu'on fait glisser la frise : la lecture ne la ramène
    /// pas en arrière.
    scrubbing: bool,
    /// Les réglages sont ouverts : l'écrou est marqué.
    settings_open: bool,
    _timeline: Subscription,
}

impl EventEmitter<BarEvent> for ReaderBar {}

impl ReaderBar {
    pub fn new(
        session: Arc<Session>,
        focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let timeline = cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.001));
        let subscription = cx.subscribe_in(
            &timeline,
            window,
            |bar: &mut Self, _, event: &SliderEvent, window, cx| match event {
                SliderEvent::Change(_) => bar.scrubbing = true,
                SliderEvent::Release(value) => {
                    bar.scrubbing = false;
                    if let Some(sentence) = sentence_at(value.end(), bar.sentence_count) {
                        bar.send(Command::SeekTo(sentence));
                    }
                    bar.focus.focus(window, cx);
                }
            },
        );
        ReaderBar {
            session,
            focus,
            snapshot: Snapshot::default(),
            sentence_count: 0,
            timeline,
            scrubbing: false,
            settings_open: false,
            _timeline: subscription,
        }
    }

    /// Le nouvel état du lecteur ; ne redessine que ce qui a changé.
    pub fn receive(
        &mut self,
        snapshot: &Snapshot,
        sentence_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *snapshot == self.snapshot && sentence_count == self.sentence_count {
            return;
        }
        let progress = snapshot.progress.clamp(0.0, 1.0);
        if !self.scrubbing && (progress - self.timeline.read(cx).value().end()).abs() > 1e-4 {
            self.timeline
                .update(cx, |state, cx| state.set_value(progress, window, cx));
        }
        self.snapshot = snapshot.clone();
        self.sentence_count = sentence_count;
        cx.notify();
    }

    /// Marque l'écrou quand les réglages sont ouverts.
    pub fn set_settings_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.settings_open != open {
            self.settings_open = open;
            cx.notify();
        }
    }

    fn send(&self, command: Command) {
        self.session.send(Request::Command(command));
    }

    fn icon_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: String,
        command: Command,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = self.snapshot.has_document;
        div().debug_selector(|| id.to_string()).child(
            Button::new(id)
                .ghost()
                .icon(icon)
                .with_size(px(16.0))
                .size(px(TARGET))
                .rounded(px(10.0))
                .tab_stop(false)
                .tooltip(tooltip)
                .disabled(!enabled)
                .on_click(cx.listener(move |bar, _, _, _| bar.send(command.clone()))),
        )
    }

    fn play_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let s = &self.snapshot;
        let (icon, label) = if s.playing {
            (IconName::Pause, dysflow::tr!("Pause", "Pause"))
        } else {
            (IconName::Play, dysflow::tr!("Play", "Lecture"))
        };
        let disc = ButtonCustomVariant::new(cx)
            .color(p.accent())
            .foreground(p.on_accent())
            .hover(p.accent().opacity(0.85))
            .active(p.accent().opacity(0.7));
        div().debug_selector(|| "transport-play".to_string()).child(
            Button::new("transport-play")
                .custom(disc)
                .icon(icon)
                .with_size(px(18.0))
                .size(px(DISC))
                .rounded_full()
                .tab_stop(false)
                .tooltip(format!("{label} ({})", dysflow::tr!("Space", "Espace")))
                .disabled(!can_play(s))
                .on_click(cx.listener(|bar, _, _, _| bar.send(Command::TogglePlay))),
        )
    }

    fn speed_menu(&self) -> impl IntoElement {
        let current = self.snapshot.speed;
        let session = Arc::clone(&self.session);
        let focus = self.focus.clone();
        div()
            .debug_selector(|| "transport-speed".to_string())
            .child(
                Button::new("transport-speed")
                    .ghost()
                    .label(SharedString::from(speed(current)))
                    .h(px(TARGET))
                    .min_w(px(52.0))
                    .rounded(px(10.0))
                    .font_weight(FontWeight::MEDIUM)
                    .tab_stop(false)
                    .tooltip(dysflow::tr!("Speed (+ −)", "Vitesse (+ −)"))
                    .disabled(!self.snapshot.has_document)
                    .dropdown_menu(move |menu, _, _| {
                        SPEEDS.iter().fold(menu, |menu, &value| {
                            let session = Arc::clone(&session);
                            let focus = focus.clone();
                            let label = speed(value);
                            menu.item(
                                // Un élément à soi, pour que l'essai le trouve
                                // (`speed-1.25`) ; le menu garde ses états.
                                PopupMenuItem::element(move |_, _| {
                                    div()
                                        .debug_selector(|| format!("speed-{value}"))
                                        .child(label.clone())
                                })
                                .checked((value - current).abs() < 0.001)
                                .on_click(move |_, window, cx| {
                                    session.send(Request::Command(Command::SetSpeed(value)));
                                    focus.focus(window, cx);
                                }),
                            )
                        })
                    }),
            )
    }
}

impl Render for ReaderBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let s = self.snapshot.clone();
        let enabled = s.has_document;
        let figures = |text: String| {
            div()
                .flex_none()
                .min_w(px(40.0))
                .text_color(p.muted())
                .font_weight(FontWeight::MEDIUM)
                // Les chiffres ne dansent pas d'une seconde à l'autre : ceux
                // à chasse fixe de la police de l'interface (embarquée,
                // partout la même), pas une police du système.
                .font_features(crate::settings::tabular())
                .text_size(px(12.0))
                .child(text)
        };
        let previous = self.icon_button(
            "transport-previous",
            IconName::SkipBack,
            dysflow::tr!("Previous sentence (←)", "Phrase précédente (←)").into(),
            Command::Sentences(-1),
            cx,
        );
        let next = self.icon_button(
            "transport-next",
            IconName::SkipForward,
            dysflow::tr!("Next sentence (→)", "Phrase suivante (→)").into(),
            Command::Sentences(1),
            cx,
        );
        let play = self.play_button(cx);
        let speed = self.speed_menu();
        let settings = div().debug_selector(|| "settings".to_string()).child(
            Button::new("settings")
                .ghost()
                .icon(IconName::Settings)
                .with_size(px(16.0))
                .size(px(TARGET))
                .rounded(px(10.0))
                .tab_stop(false)
                .selected(self.settings_open)
                .tooltip(format!(
                    "{} ({})",
                    dysflow::tr!("Settings", "Réglages"),
                    crate::inputs::command_label(",")
                ))
                .on_click(cx.listener(|_, _, _, cx| cx.emit(BarEvent::Settings))),
        );
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(22.0))
            .bg(p.surface())
            .text_size(px(13.0))
            .text_color(p.chrome())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(previous)
                    .child(play)
                    .child(next),
            )
            .child(figures(time(s.elapsed)))
            .child(
                div().flex_1().min_w(px(80.0)).child(
                    Slider::new(&self.timeline)
                        .tab_stop(false)
                        .disabled(!enabled)
                        .bg(p.accent())
                        .text_color(p.accent())
                        .accessibility_label(dysflow::tr!("Progress", "Progression")),
                ),
            )
            .child(figures(remaining(s.elapsed, s.total)))
            .child(speed)
            .child(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(time(Some(125.4)), "2:05");
        assert_eq!(time(Some(-3.0)), "0:00");
        assert_eq!(time(None), "–:––");
        assert_eq!(time(Some(f64::NAN)), "–:––");
        assert_eq!(remaining(Some(56.0), Some(176.0)), "-2:00");
        assert_eq!(remaining(Some(200.0), Some(176.0)), "-0:00");
        assert_eq!(remaining(None, Some(176.0)), "–:––");
    }

    #[test]
    fn speeds() {
        let fr = dysflow::i18n::ui_lang() == dysflow::i18n::UiLang::Fr;
        let sep = if fr { "," } else { "." };
        assert_eq!(speed(1.0), "1×");
        assert_eq!(speed(1.25), format!("1{sep}25×"));
        assert_eq!(speed(1.1), format!("1{sep}1×"));
        assert_eq!(speed(0.75), format!("0{sep}75×"));
    }

    #[test]
    fn timeline_fraction_to_sentence() {
        assert_eq!(sentence_at(0.0, 10), Some(0));
        assert_eq!(sentence_at(0.55, 10), Some(5));
        assert_eq!(sentence_at(1.0, 10), Some(9));
        assert_eq!(sentence_at(2.0, 10), Some(9));
        assert_eq!(sentence_at(0.5, 0), None);
        assert_eq!(sentence_at(f32::NAN, 10), None);
    }
}

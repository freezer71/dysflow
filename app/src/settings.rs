//! La page des réglages : un panneau à droite de la lecture, sur un seul
//! écran qui défile, dans l'ordre du brief — la voix (le point fort de la
//! page, avec « Écouter »), la vitesse, le thème, la police (famille et
//! taille), la langue de l'interface.
//!
//! Le texte reste visible à gauche : une police, une taille ou un thème
//! changés s'y voient aussitôt. Chaque changement s'enregistre tout de suite
//! (`Reader::update_settings`, `config.toml`) : il n'y a pas de bouton
//! « Enregistrer ». Rien ne bloque le fil de l'interface : chaque requête
//! part au lecteur sans attendre (`Session::post`, dans l'ordre des gestes),
//! sa réponse s'attend sur l'exécuteur d'arrière-plan et revient par
//! `update`. La fenêtre reçoit `SettingsEvent::Saved` et applique le reste
//! (vue de lecture, thème, langue de l'interface).
//!
//! Tout se fait au clavier : Tab parcourt les contrôles (focus visible,
//! bleu), Entrée ou Espace les actionnent, ← → déplacent les curseurs,
//! Échap ferme la page.

use std::sync::Arc;
use std::time::Instant;

use dysflow::config::{Theme, READING_FONTS, READING_SIZE_MAX, READING_SIZE_MIN};
use dysflow::i18n::UiLang;
use dysflow::reader::settings::{SettingsPatch, SettingsView, VoiceChoice, VoiceModel};
use dysflow::reader::view::Snapshot;
use dysflow::tr;
use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, FontWeight, IntoElement,
    KeyDownEvent, SharedString, Subscription, Window,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::{Disableable, Icon, Selectable, Sizable};
use gpui_kit_assets::IconName;

use crate::fonts;
use crate::session::Session;
use crate::theme::{self, Palette};
use crate::transport::speed as speed_label;

/// La largeur du panneau.
pub const WIDTH: f32 = 380.0;
/// Les bornes de la vitesse, celles du cœur (`Config::normalised`).
const SPEED_MIN: f32 = 0.5;
const SPEED_MAX: f32 = 3.0;

/// La langue de l'interface d'un réglage `ui_language` : vide, celle du
/// système (voir `ui_lang_choice`), lue dans l'environnement du processus
/// puis auprès de la plateforme (`sys-locale`).
pub fn ui_lang_for(code: &str) -> UiLang {
    ui_lang_choice(code, |k| std::env::var(k).ok(), sys_locale::get_locale)
}

/// Vide, « C » ou « POSIX » (« C.UTF-8 » compris) : la locale neutre, qui ne
/// dit aucune langue ; elle compte comme absente.
fn names_a_language(tag: &str) -> bool {
    let code = tag.split(['.', '@']).next().unwrap_or("");
    !(code.is_empty() || code == "C" || code == "POSIX")
}

/// Le choix de la langue de l'interface, sans rien lire lui-même. « fr » et
/// « en » s'imposent. Sinon (« Système ») : les variables de locale
/// (`LC_ALL`, `LC_MESSAGES`, `LANG`, comme le terminal), la première qui
/// nomme une langue (ni vide, ni « C », ni « POSIX ») ; à défaut la langue de la plateforme (`platform`,
/// lue seulement alors) : une app lancée du Finder ou sous Windows n'a
/// souvent pas `LANG`, et sortirait en anglais pour un francophone. Ni
/// l'un ni l'autre : l'anglais.
pub fn ui_lang_choice(
    code: &str,
    env: impl Fn(&str) -> Option<String>,
    platform: impl FnOnce() -> Option<String>,
) -> UiLang {
    match code {
        "fr" => return UiLang::Fr,
        "en" => return UiLang::En,
        _ => {}
    }
    let from_env = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| env(k))
        .find(|v| names_a_language(v));
    match from_env.or_else(|| platform().filter(|v| names_a_language(v))) {
        Some(tag) => UiLang::from_locale(&tag),
        None => UiLang::En,
    }
}

/// Ce que la page lit du lecteur à l'ouverture.
#[derive(Debug, Clone)]
struct Loaded {
    values: SettingsView,
    voices: Vec<VoiceChoice>,
    family: String,
    models: Vec<VoiceModel>,
    /// La langue des textes par défaut (« auto » résolu).
    language: String,
}

/// Ce que la page dit à la fenêtre.
#[derive(Debug, Clone)]
pub enum SettingsEvent {
    /// Un changement est enregistré : les réglages tels qu'appliqués.
    Saved(Box<SettingsView>),
    /// Échap ou le bouton de fermeture.
    Close,
}

pub struct SettingsPage {
    session: Arc<Session>,
    focus: FocusHandle,
    loaded: Option<Loaded>,
    /// La langue dont on choisit la voix (« fr », « en »).
    voice_language: String,
    snapshot: Snapshot,
    /// Le refus d'« Écouter », dit par le lecteur.
    preview_note: Option<String>,
    /// La dernière erreur d'enregistrement, d'installation ou de lecture.
    error: Option<String>,
    speed: Entity<SliderState>,
    size: Entity<SliderState>,
    /// Les tuiles des polices, chacune un arrêt de Tab.
    font_focus: [FocusHandle; 3],
    /// Les curseurs reprennent la valeur enregistrée au prochain rendu.
    sync_sliders: bool,
    /// Un curseur est tenu : la lecture ne le ramène pas en arrière.
    dragging: bool,
    /// Numéro du dernier changement envoyé, et du dernier dont la réponse
    /// a été prise : une réponse plus ancienne n'écrase pas l'affichage.
    sent: u64,
    applied: u64,
    _sliders: [Subscription; 2],
    _load: gpui::Task<()>,
}

impl EventEmitter<SettingsEvent> for SettingsPage {}

impl SettingsPage {
    pub fn new(session: Arc<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let speed = cx.new(|_| {
            SliderState::new()
                .min(SPEED_MIN)
                .max(SPEED_MAX)
                .step(0.05)
                .default_value(1.0)
        });
        let size = cx.new(|_| {
            SliderState::new()
                .min(READING_SIZE_MIN)
                .max(READING_SIZE_MAX)
                .step(1.0)
                .default_value(dysflow::config::READING_SIZE_DEFAULT)
        });
        let speed_events = cx.subscribe_in(
            &speed,
            window,
            |this: &mut Self, _, event: &SliderEvent, _, cx| match event {
                // Pendant le geste, seul le chiffre suit ; au lâcher (ou à
                // chaque cran du clavier), la valeur s'enregistre.
                SliderEvent::Change(value) => this.show(|v| v.speed = round2(value.end()), cx),
                SliderEvent::Release(value) => {
                    this.dragging = false;
                    this.set_speed(value.end(), cx)
                }
            },
        );
        let size_events = cx.subscribe_in(
            &size,
            window,
            |this: &mut Self, _, event: &SliderEvent, _, cx| match event {
                SliderEvent::Change(value) => {
                    this.show(|v| v.reading_size = value.end().round(), cx)
                }
                SliderEvent::Release(value) => {
                    this.dragging = false;
                    this.set_reading_size(value.end(), cx)
                }
            },
        );
        SettingsPage {
            snapshot: session.snapshot(),
            session,
            focus: cx.focus_handle(),
            loaded: None,
            voice_language: String::new(),
            preview_note: None,
            error: None,
            speed,
            size,
            font_focus: [(); 3].map(|_| cx.focus_handle().tab_stop(true)),
            sync_sliders: false,
            dragging: false,
            sent: 0,
            applied: 0,
            _sliders: [speed_events, size_events],
            _load: gpui::Task::ready(()),
        }
    }

    /// Le titre de la page, dans la langue de l'interface.
    pub fn title(&self) -> &'static str {
        tr!("Settings", "Réglages")
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// La page prend le clavier (Tab passe ensuite d'un contrôle à l'autre).
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    /// Relit réglages, voix et modèles, sans bloquer : la question attend le
    /// lecteur sur l'exécuteur d'arrière-plan.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let reply = self.session.post(|reader| {
            let capabilities = reader.capabilities();
            Loaded {
                values: reader.settings(),
                voices: capabilities.voices,
                family: capabilities.family,
                models: reader
                    .voices()
                    .into_iter()
                    .filter(|m| m.kind == "speech")
                    .collect(),
                language: reader.config().default_language(),
            }
        });
        self._load = cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { reply.wait() })
                .await;
            let _ = this.update(cx, |page, cx| match answer {
                Some(loaded) => page.receive_loaded(loaded, cx),
                None => {
                    page.error = Some(no_answer());
                    cx.notify();
                }
            });
        });
    }

    fn receive_loaded(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        if self.voice_language.is_empty() {
            let chosen = loaded.values.language.as_str();
            self.voice_language = if matches!(chosen, "fr" | "en") {
                chosen.to_string()
            } else if loaded.language == "en" {
                "en".into()
            } else {
                "fr".into()
            };
        }
        self.loaded = Some(loaded);
        self.sync_sliders = true;
        cx.notify();
    }

    /// Les réglages sont lus.
    pub fn loaded(&self) -> bool {
        self.loaded.is_some()
    }

    /// Les réglages tels que la page les montre.
    pub fn values(&self) -> Option<&SettingsView> {
        self.loaded.as_ref().map(|l| &l.values)
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Les modèles de lecture du catalogue (la voix du système comprise).
    pub fn models(&self) -> Vec<VoiceModel> {
        self.loaded
            .as_ref()
            .map(|l| l.models.clone())
            .unwrap_or_default()
    }

    pub fn size_slider(&self) -> &Entity<SliderState> {
        &self.size
    }

    pub fn speed_slider(&self) -> &Entity<SliderState> {
        &self.speed
    }

    /// Le nouvel instantané du lecteur : la vitesse changée au clavier (+ −),
    /// l'aperçu, un téléchargement qui avance ou finit.
    pub fn receive(&mut self, snapshot: &Snapshot, cx: &mut Context<Self>) {
        if *snapshot == self.snapshot {
            return;
        }
        let finished = self.snapshot.download.is_some() && snapshot.download.is_none();
        let speed = snapshot.speed;
        self.snapshot = snapshot.clone();
        // Pas pendant un geste ni avant la réponse d'un changement : la
        // vitesse publiée y est encore l'ancienne.
        let settled = !self.dragging && self.sent == self.applied;
        if let Some(values) = self
            .loaded
            .as_mut()
            .map(|l| &mut l.values)
            .filter(|_| settled)
        {
            if (values.speed - speed).abs() > 0.001 && speed > 0.0 {
                values.speed = speed;
                self.sync_sliders = true;
            }
        }
        if finished && self.loaded.is_some() {
            // Une voix installée : sa ligne change, ses voix arrivent.
            self.load(cx);
        }
        cx.notify();
    }

    // La voix.

    /// La langue dont on choisit la voix.
    pub fn voice_language(&self) -> &str {
        &self.voice_language
    }

    pub fn set_voice_language(&mut self, language: &str, cx: &mut Context<Self>) {
        self.voice_language = language.to_string();
        self.preview_note = None;
        cx.notify();
    }

    /// Les voix proposées : celles du modèle, dans la langue choisie.
    pub fn voice_options(&self) -> Vec<VoiceChoice> {
        let Some(loaded) = &self.loaded else {
            return Vec::new();
        };
        loaded
            .voices
            .iter()
            .filter(|v| v.language == self.voice_language)
            .cloned()
            .collect()
    }

    /// La voix en place pour la langue choisie. Voix du système : celle des
    /// réglages ; sinon celle de la langue, ou la première de la liste
    /// (celle que le cœur prend par défaut).
    pub fn current_voice(&self) -> String {
        let Some(loaded) = &self.loaded else {
            return String::new();
        };
        let options = self.voice_options();
        let chosen = if loaded.family == "system" {
            Some(loaded.values.voice.clone())
        } else {
            loaded
                .values
                .voices_by_language
                .get(&self.voice_language)
                .cloned()
        };
        chosen
            .filter(|id| options.iter().any(|v| &v.id == id))
            .or_else(|| options.first().map(|v| v.id.clone()))
            .unwrap_or_default()
    }

    pub fn choose_voice(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(loaded) = &self.loaded else { return };
        let patch = if loaded.family == "system" {
            SettingsPatch {
                voice: Some(id.to_string()),
                ..Default::default()
            }
        } else {
            SettingsPatch {
                voice_for_language: Some((self.voice_language.clone(), id.to_string())),
                ..Default::default()
            }
        };
        self.preview_note = None;
        self.save(patch, cx);
    }

    /// « Écouter » : la phrase d'exemple, avec la voix et la vitesse en
    /// place, dans la langue choisie.
    pub fn listen(&mut self, cx: &mut Context<Self>) {
        let language = self.voice_language.clone();
        let reply = self
            .session
            .post(move |reader| reader.preview_voice_in(&language, Instant::now()));
        self.preview_note = None;
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { reply.wait() })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.preview_note = match answer {
                    Some(Ok(())) => None,
                    Some(Err(why)) => Some(why),
                    None => Some(no_answer()),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn stop_listening(&mut self, cx: &mut Context<Self>) {
        self.session
            .send(dysflow::reader::handle::Request::Ask(Box::new(|reader| {
                reader.stop_preview()
            })));
        cx.notify();
    }

    fn previewing(&self) -> bool {
        self.snapshot.preview.pending || self.snapshot.preview.playing
    }

    /// Ce que dit l'aperçu : son refus, son échec, ou ce qu'il fait.
    pub fn preview_line(&self) -> Option<String> {
        if let Some(note) = &self.preview_note {
            return Some(note.clone());
        }
        let preview = &self.snapshot.preview;
        if let Some(error) = &preview.error {
            return Some(error.clone());
        }
        if preview.pending || preview.preparing {
            return Some(tr!("Preparing the preview…", "Préparation de l'aperçu…").into());
        }
        if preview.playing {
            return Some(tr!("Listening…", "Écoute en cours…").into());
        }
        None
    }

    /// Installe une voix du catalogue : le téléchargement se suit dans la
    /// page et la barre d'état.
    pub fn install(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_string();
        let reply = self
            .session
            .post(move |reader| reader.install_voice(&id).map_err(|e| format!("{e:#}")));
        self.answer(reply, false, cx);
    }

    /// Lit avec une voix installée : le lecteur la charge à la même phrase.
    pub fn use_model(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_string();
        let reply = self.session.post(move |reader| {
            reader
                .use_voice(&id, Instant::now())
                .map_err(|e| format!("{e:#}"))
        });
        self.answer(reply, true, cx);
    }

    /// Attend la réponse d'une action sur les voix ; `reload` relit tout
    /// ensuite (les voix proposées changent avec le modèle).
    fn answer(
        &mut self,
        reply: crate::session::Reply<Result<(), String>>,
        reload: bool,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { reply.wait() })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.error = match answer {
                    Some(Ok(())) => None,
                    Some(Err(why)) => Some(why),
                    None => Some(no_answer()),
                };
                if reload {
                    page.load(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // Les autres réglages.

    pub fn set_speed(&mut self, value: f32, cx: &mut Context<Self>) {
        let speed = round2(value.clamp(SPEED_MIN, SPEED_MAX));
        self.save(
            SettingsPatch {
                speed: Some(speed),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.save(
            SettingsPatch {
                theme: Some(theme),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn set_reading_font(&mut self, font: &str, cx: &mut Context<Self>) {
        self.save(
            SettingsPatch {
                reading_font: Some(font.to_string()),
                ..Default::default()
            },
            cx,
        );
    }

    /// La taille du texte, ramenée entre 14 et 40 points, au point près.
    pub fn set_reading_size(&mut self, value: f32, cx: &mut Context<Self>) {
        let size = if value.is_finite() {
            value.round().clamp(READING_SIZE_MIN, READING_SIZE_MAX)
        } else {
            dysflow::config::READING_SIZE_DEFAULT
        };
        self.save(
            SettingsPatch {
                reading_size: Some(size),
                ..Default::default()
            },
            cx,
        );
    }

    /// « », « fr » ou « en ».
    pub fn set_ui_language(&mut self, code: &str, cx: &mut Context<Self>) {
        self.save(
            SettingsPatch {
                ui_language: Some(code.to_string()),
                ..Default::default()
            },
            cx,
        );
    }

    /// Montre une valeur sans l'enregistrer (un curseur qu'on fait glisser).
    fn show(&mut self, change: impl FnOnce(&mut SettingsView), cx: &mut Context<Self>) {
        self.dragging = true;
        if let Some(loaded) = self.loaded.as_mut() {
            change(&mut loaded.values);
            cx.notify();
        }
    }

    /// Enregistre un changement : l'affichage suit tout de suite, la requête
    /// part au lecteur dans l'ordre des gestes, sa réponse revient par
    /// `update` et la fenêtre en est prévenue.
    fn save(&mut self, patch: SettingsPatch, cx: &mut Context<Self>) {
        if let Some(loaded) = self.loaded.as_mut() {
            preview_patch(&mut loaded.values, &patch);
            self.sync_sliders = true;
        }
        self.sent += 1;
        let sent = self.sent;
        let reply =
            self.session.post(
                move |reader| match reader.update_settings(&patch, Instant::now()) {
                    Ok(applied) => (applied, None),
                    Err(e) => (reader.settings(), Some(format!("{e:#}"))),
                },
            );
        // Sans délai : un réglage envoyé s'applique toujours, même quand le
        // lecteur est occupé longtemps ; la page dit « Enregistrement… »
        // en attendant.
        cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { reply.wait_unbounded() })
                .await;
            let _ = this.update(cx, |page, cx| page.saved(sent, answer, cx));
        })
        .detach();
        cx.notify();
    }

    /// Un changement attend encore la réponse du lecteur.
    pub fn saving(&self) -> bool {
        self.applied < self.sent
    }

    /// Le numéro du dernier changement envoyé.
    pub fn last_sent(&self) -> u64 {
        self.sent
    }

    /// Fait arriver la réponse du changement n° `sent`, comme le lecteur la
    /// rendrait. Pour les essais : l'ordre des réponses n'y est pas garanti
    /// (une tâche d'arrière-plan par changement), il faut pouvoir le forcer.
    #[doc(hidden)]
    pub fn deliver_saved(&mut self, sent: u64, applied: SettingsView, cx: &mut Context<Self>) {
        self.saved(sent, Some((applied, None)), cx);
    }

    fn saved(
        &mut self,
        sent: u64,
        answer: Option<(SettingsView, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        // Une réponse plus ancienne que la dernière prise ne change rien :
        // ni la page, ni la fenêtre (thème, langue, lecture).
        let newest = sent >= self.applied;
        let Some((applied, failure)) = answer else {
            // Le canal s'est fermé : le lecteur s'est arrêté.
            self.error = Some(
                tr!(
                    "The reader has stopped: this setting was not applied",
                    "Le lecteur s'est arrêté : ce réglage n'est pas appliqué"
                )
                .to_string(),
            );
            if newest {
                self.applied = sent;
            }
            cx.notify();
            return;
        };
        match failure {
            // Le réglage est appliqué, mais pas écrit : on le dit.
            Some(why) => {
                self.error = Some(tr!(
                    en = "The setting applies but could not be saved: {}",
                    fr = "Le réglage s'applique mais n'a pas pu s'enregistrer : {}",
                    why
                ))
            }
            None if newest => self.error = None,
            None => {}
        }
        if newest {
            self.applied = sent;
            if let Some(loaded) = self.loaded.as_mut() {
                loaded.values = applied.clone();
            }
            self.sync_sliders = true;
            cx.emit(SettingsEvent::Saved(Box::new(applied)));
        }
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            cx.emit(SettingsEvent::Close);
        }
    }
}

fn no_answer() -> String {
    tr!(
        "The reader did not answer: try again in a moment",
        "Le lecteur n'a pas répondu : réessaie dans un instant"
    )
    .to_string()
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// Ce que l'affichage montre d'un changement, avant la réponse du lecteur.
fn preview_patch(values: &mut SettingsView, patch: &SettingsPatch) {
    if let Some(speed) = patch.speed {
        values.speed = speed;
    }
    if let Some(theme) = patch.theme {
        values.theme = theme;
    }
    if let Some(font) = &patch.reading_font {
        values.reading_font = font.clone();
    }
    if let Some(size) = patch.reading_size {
        values.reading_size = size;
    }
    if let Some(code) = &patch.ui_language {
        values.ui_language = code.clone();
    }
    if let Some(voice) = &patch.voice {
        values.voice = voice.clone();
    }
    if let Some((language, voice)) = &patch.voice_for_language {
        values
            .voices_by_language
            .insert(language.clone(), voice.clone());
    }
}

// Le dessin.

/// Un titre de section : petit, en capitales douces, au-dessus de sa carte.
fn section_title(p: &Palette, text: &'static str) -> impl IntoElement {
    div()
        .px(px(4.0))
        .pt(px(18.0))
        .pb(px(8.0))
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.muted())
        .child(text)
}

/// Une carte : les réglages d'un groupe, séparés par un filet.
fn card(p: &Palette) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(14.0))
        .bg(p.card())
        .border_1()
        .border_color(p.border())
}

/// Une ligne de réglage : son libellé (et sa valeur à droite), son contrôle
/// dessous, sur toute la largeur.
fn row(p: &Palette, first: bool) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .px(px(16.0))
        .py(px(14.0))
        .when(!first, |this| this.border_t_1().border_color(p.border()))
}

fn label_line(p: &Palette, label: &'static str, value: Option<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.text())
                .child(label),
        )
        .children(value.map(|value| {
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.chrome())
                .font_features(tabular())
                .child(value)
        }))
}

fn detail(p: &Palette, text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(12.0))
        .text_color(p.muted())
        .child(text.into())
}

/// Les chiffres à chasse fixe d'Atkinson Hyperlegible : une valeur qui
/// change ne fait pas danser la ligne.
pub fn tabular() -> gpui::FontFeatures {
    gpui::FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// Un choix parmi quelques-uns (thème, langue) : des segments côte à côte,
/// le choisi posé en carte, en gras et coché (pas seulement par la couleur).
fn segments<T: Copy + PartialEq + 'static>(
    p: &'static Palette,
    id: &'static str,
    options: Vec<(T, &'static str, Option<IconName>)>,
    current: T,
    cx: &mut Context<SettingsPage>,
    choose: fn(&mut SettingsPage, T, &mut Context<SettingsPage>),
) -> impl IntoElement {
    let chosen = ButtonCustomVariant::new(cx)
        .color(p.card())
        .foreground(p.text())
        .hover(p.card())
        .active(p.card())
        .shadow(true);
    div()
        .flex()
        .gap(px(2.0))
        .p(px(3.0))
        .rounded(px(10.0))
        .bg(p.hover())
        .children(
            options
                .into_iter()
                .enumerate()
                .map(|(i, (value, label, icon))| {
                    let selected = value == current;
                    let mut button = Button::new((id, i))
                        .label(label)
                        .with_size(gpui_component::Size::Small)
                        .h(px(30.0))
                        .flex_1()
                        .rounded(px(8.0))
                        .selected(selected)
                        .on_click(cx.listener(move |page, _, _, cx| choose(page, value, cx)));
                    button = if selected {
                        button
                            .custom(chosen)
                            .font_weight(FontWeight::SEMIBOLD)
                            .icon(Icon::new(IconName::Check).with_size(px(14.0)))
                    } else {
                        let button = button.ghost();
                        match icon {
                            Some(icon) => button.icon(Icon::new(icon).with_size(px(14.0))),
                            None => button,
                        }
                    };
                    div()
                        .flex_1()
                        .debug_selector(move || format!("{id}-{i}"))
                        .child(button)
                }),
        )
}

impl SettingsPage {
    fn header(&self, p: &'static Palette, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .h(px(56.0))
            .pl(px(20.0))
            .pr(px(12.0))
            .border_b_1()
            .border_color(p.border())
            .child(
                div()
                    .flex_1()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text())
                    .child(self.title()),
            )
            // Discret : le lecteur n'a pas encore répondu (il peut être
            // occupé longtemps) ; le réglage s'appliquera dès sa réponse.
            .when(self.saving(), |this| {
                this.child(
                    div()
                        .debug_selector(|| "settings-saving".to_string())
                        .text_size(px(12.0))
                        .text_color(p.muted())
                        .child(tr!("Saving…", "Enregistrement…")),
                )
            })
            .child(
                div().debug_selector(|| "settings-close".to_string()).child(
                    Button::new("settings-close")
                        .ghost()
                        .icon(Icon::new(IconName::Close).with_size(px(16.0)))
                        .size(px(32.0))
                        .rounded(px(10.0))
                        .tooltip(format!(
                            "{} ({})",
                            tr!("Close the settings", "Fermer les réglages"),
                            tr!("Esc", "Échap")
                        ))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close))),
                ),
            )
    }

    /// La voix : le point fort de la page. Sa langue, la voix elle-même en
    /// grand, « Écouter » (seul bouton plein de la page), le modèle et la
    /// vitesse.
    fn voice_card(&self, p: &'static Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let options = self.voice_options();
        let current = self.current_voice();
        let current_label = options
            .iter()
            .find(|v| v.id == current)
            .map(|v| v.label.clone())
            .unwrap_or_else(|| tr!("Default voice", "Voix par défaut").to_string());
        let previewing = self.previewing();
        let no_options = options.is_empty();
        let listen_fill = ButtonCustomVariant::new(cx)
            .color(p.accent())
            .foreground(p.on_accent())
            .hover(p.accent().opacity(0.88))
            .active(p.accent().opacity(0.76));
        let listen = if previewing {
            Button::new("settings-stop")
                .custom(listen_fill)
                .icon(Icon::new(IconName::Square).with_size(px(14.0)))
                .label(tr!("Stop", "Arrêter"))
                .on_click(cx.listener(|page, _, _, cx| page.stop_listening(cx)))
        } else {
            Button::new("settings-listen")
                .custom(listen_fill)
                .icon(Icon::new(IconName::Play).with_size(px(14.0)))
                .label(tr!("Listen", "Écouter"))
                .tooltip(tr!(
                    "Hear a sample sentence with this voice",
                    "Entendre une phrase d'exemple avec cette voix"
                ))
                .on_click(cx.listener(|page, _, _, cx| page.listen(cx)))
        }
        .h(px(40.0))
        .px(px(16.0))
        .rounded(px(10.0))
        .font_weight(FontWeight::SEMIBOLD);
        let entity = cx.entity();
        let picker = Button::new("settings-voice")
            .outline()
            .h(px(40.0))
            .w_full()
            .rounded(px(10.0))
            .justify_between()
            .disabled(no_options)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text())
                    .child(current_label),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .with_size(px(16.0))
                    .text_color(p.chrome()),
            )
            .accessibility_label(tr!("Voice", "Voix"))
            .dropdown_menu(move |menu, _, _| {
                let menu = menu.max_h(px(320.0)).scrollable(true).min_w(px(300.0));
                options.iter().fold(menu, |menu, voice| {
                    let id = voice.id.clone();
                    let entity = entity.clone();
                    menu.item(
                        PopupMenuItem::new(voice.label.clone())
                            .checked(voice.id == current)
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |page, cx| page.choose_voice(&id, cx));
                            }),
                    )
                })
            });
        let languages = segments(
            p,
            "voice-language",
            vec![
                ("fr", tr!("French", "Français"), None),
                ("en", tr!("English", "Anglais"), None),
            ],
            if self.voice_language == "en" {
                "en"
            } else {
                "fr"
            },
            cx,
            |page, language, cx| page.set_voice_language(language, cx),
        );
        let preview = self.preview_line().map(|line| {
            let failed = self.preview_note.is_some() || self.snapshot.preview.error.is_some();
            div()
                .debug_selector(|| "settings-preview".to_string())
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(12.0))
                .text_color(if failed { p.error() } else { p.muted() })
                .when(failed, |this| {
                    this.child(Icon::new(IconName::CircleAlert).with_size(px(14.0)))
                })
                .child(line)
        });
        let speed = self.values().map_or(1.0, |v| v.speed);
        card(p)
            .debug_selector(|| "settings-voice-card".to_string())
            .child(
                row(p, true)
                    .gap(px(12.0))
                    .child(label_line(p, tr!("Voice", "Voix"), None))
                    .child(languages)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().flex_1().min_w(px(0.0)).child(picker))
                            .child(
                                div()
                                    .debug_selector(|| "settings-listen".to_string())
                                    .child(listen),
                            ),
                    )
                    .children(preview)
                    .when(no_options && self.loaded(), |this| {
                        this.child(detail(
                            p,
                            tr!(
                                "This voice has no choice for this language.",
                                "Cette voix n'offre pas de choix pour cette langue."
                            ),
                        ))
                    }),
            )
            .child(self.models_row(p, cx))
            .child(
                row(p, false)
                    .child(label_line(
                        p,
                        tr!("Speed", "Vitesse"),
                        Some(speed_label(speed).into()),
                    ))
                    .child(
                        div()
                            .debug_selector(|| "settings-speed".to_string())
                            .px(px(6.0))
                            .child(
                                Slider::new(&self.speed).accessibility_label(tr!(
                                    "Reading speed",
                                    "Vitesse de lecture"
                                )),
                            ),
                    )
                    .child(detail(
                        p,
                        tr!(
                            "In the reader: + and −, or the speed in the bar",
                            "Dans la lecture : + et −, ou la vitesse de la barre"
                        ),
                    )),
            )
    }

    /// Le modèle de voix : la voix du système et celles du catalogue, avec
    /// ce qu'on peut en faire (utiliser, installer) ou ce qu'elles font.
    fn models_row(&self, p: &'static Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let download = self.snapshot.download.clone();
        let models = self.models();
        let rows = models.into_iter().enumerate().map(|(i, m)| {
            let downloading = download
                .as_ref()
                .filter(|d| d.id == m.id && d.failed.is_none());
            let size = dysflow::models::human_size(m.bytes);
            let what = if m.system {
                m.languages_label.clone()
            } else {
                format!("{} · {}", m.languages_label, size)
            };
            let action: gpui::AnyElement = if m.active {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text())
                    .child(Icon::new(IconName::CircleCheck).with_size(px(14.0)))
                    .child(tr!("In use", "Utilisée"))
                    .into_any_element()
            } else if let Some(d) = downloading {
                let percent = if d.total > 0 {
                    (d.received as f64 / d.total as f64 * 100.0).round() as u32
                } else {
                    0
                };
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.chrome())
                    .font_features(tabular())
                    .child(format!("{percent} %"))
                    .into_any_element()
            } else if m.installed {
                let id = m.id.clone();
                Button::new(("settings-use", i))
                    .outline()
                    .with_size(gpui_component::Size::Small)
                    .label(tr!("Use", "Utiliser"))
                    .on_click(cx.listener(move |page, _, _, cx| page.use_model(&id, cx)))
                    .into_any_element()
            } else {
                let id = m.id.clone();
                Button::new(("settings-install", i))
                    .outline()
                    .with_size(gpui_component::Size::Small)
                    .icon(Icon::new(IconName::Download).with_size(px(14.0)))
                    .label(tr!("Install", "Installer"))
                    .tooltip(tr!(
                        en = "Download {} ({})",
                        fr = "Télécharger {} ({})",
                        m.name,
                        size
                    ))
                    .disabled(download.as_ref().is_some_and(|d| d.failed.is_none()))
                    .on_click(cx.listener(move |page, _, _, cx| page.install(&id, cx)))
                    .into_any_element()
            };
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .py(px(8.0))
                .when(i > 0, |this| this.border_t_1().border_color(p.border()))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.text())
                                .child(m.name.clone())
                                .when(m.recommended && !m.active, |this| {
                                    this.child(
                                        div()
                                            .px(px(6.0))
                                            .rounded(px(6.0))
                                            .bg(p.keycap())
                                            .text_size(px(11.0))
                                            .text_color(p.chrome())
                                            .child(tr!("Recommended", "Recommandée")),
                                    )
                                }),
                        )
                        .child(detail(p, what)),
                )
                .child(div().flex_none().child(action))
        });
        row(p, false)
            .gap(px(4.0))
            .child(label_line(p, tr!("Voice model", "Modèle de voix"), None))
            .children(rows)
    }

    fn display_card(
        &self,
        p: &'static Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let values = self.values().cloned().unwrap_or_default();
        let theme = segments(
            p,
            "settings-theme",
            vec![
                (
                    Theme::System,
                    tr!("System", "Système"),
                    Some(IconName::Monitor),
                ),
                (Theme::Light, tr!("Light", "Clair"), Some(IconName::Sun)),
                (Theme::Dark, tr!("Dark", "Sombre"), Some(IconName::Moon)),
            ],
            values.theme,
            cx,
            |page, theme, cx| page.set_theme(theme, cx),
        );
        let chosen_font = if READING_FONTS.contains(&values.reading_font.as_str()) {
            values.reading_font.clone()
        } else {
            READING_FONTS[0].to_string()
        };
        // Chaque police se montre dans son dessin. Au clavier, Tab s'arrête
        // sur chacune, Entrée ou Espace la choisit ; le focus se voit au
        // filet bleu, le choix à l'ambre, à la coche et au gras.
        let font_tiles = READING_FONTS.iter().enumerate().map(|(i, &id)| {
            let selected = chosen_font == id;
            let family = fonts::family(id);
            let handle = self.font_focus[i].clone();
            let focused = handle.is_focused(window);
            div()
                .id(("settings-font", i))
                .debug_selector(move || format!("settings-font-{id}"))
                .track_focus(&handle)
                .on_key_down(cx.listener(move |page, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        page.set_reading_font(id, cx);
                    }
                }))
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .py(px(10.0))
                .rounded(px(10.0))
                .border_2()
                .cursor_pointer()
                .border_color(if focused {
                    p.focus()
                } else if selected {
                    p.accent()
                } else {
                    p.border()
                })
                .when(selected, |this| this.bg(p.hover()))
                .when(!selected, |this| this.hover(|s| s.bg(p.hover())))
                .child(
                    div()
                        .font_family(family)
                        .text_size(px(22.0))
                        .text_color(p.text())
                        .child("Aa"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .text_size(px(12.0))
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if selected { p.text() } else { p.chrome() })
                        .when(selected, |this| {
                            this.child(Icon::new(IconName::Check).with_size(px(12.0)))
                        })
                        .child(fonts::label(id)),
                )
                .on_click(cx.listener(move |page, _, _, cx| page.set_reading_font(id, cx)))
        });
        let size = values.reading_size.round() as i32;
        card(p)
            .child(
                row(p, true)
                    .child(label_line(p, tr!("Theme", "Thème"), None))
                    .child(theme),
            )
            .child(
                row(p, false)
                    .child(label_line(p, tr!("Font", "Police"), None))
                    .child(div().flex().gap(px(8.0)).children(font_tiles)),
            )
            .child(
                row(p, false)
                    .child(label_line(
                        p,
                        tr!("Text size", "Taille du texte"),
                        Some(format!("{size} pt").into()),
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(p.chrome())
                                    .font_family(fonts::family(&chosen_font))
                                    .child("A"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "settings-size".to_string())
                                    .flex_1()
                                    .child(
                                        Slider::new(&self.size).accessibility_label(tr!(
                                            "Text size",
                                            "Taille du texte"
                                        )),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(22.0))
                                    .text_color(p.chrome())
                                    .font_family(fonts::family(&chosen_font))
                                    .child("A"),
                            ),
                    ),
            )
    }

    fn language_card(&self, p: &'static Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let code = self
            .values()
            .map(|v| v.ui_language.clone())
            .unwrap_or_default();
        let current: &'static str = match code.as_str() {
            "fr" => "fr",
            "en" => "en",
            _ => "",
        };
        card(p).child(
            row(p, true)
                .child(label_line(
                    p,
                    tr!("Interface language", "Langue de l'interface"),
                    None,
                ))
                .child(segments(
                    p,
                    "settings-ui-language",
                    vec![
                        ("", tr!("System", "Système"), Some(IconName::Monitor)),
                        // Chaque langue s'écrit dans la sienne : on la
                        // reconnaît même sans lire l'autre.
                        ("fr", "Français", None),
                        ("en", "English", None),
                    ],
                    current,
                    cx,
                    |page, code, cx| page.set_ui_language(code, cx),
                ))
                .child(detail(
                    p,
                    tr!(
                        "Menus and messages. The text you read keeps its own language.",
                        "Menus et messages. Le texte lu garde sa propre langue."
                    ),
                )),
        )
    }
}

impl Render for SettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        if self.sync_sliders {
            self.sync_sliders = false;
            if let Some(values) = self.values().cloned() {
                if (self.speed.read(cx).value().end() - values.speed).abs() > 1e-4 {
                    self.speed
                        .update(cx, |s, cx| s.set_value(values.speed, window, cx));
                }
                if (self.size.read(cx).value().end() - values.reading_size).abs() > 1e-4 {
                    self.size
                        .update(cx, |s, cx| s.set_value(values.reading_size, window, cx));
                }
            }
        }
        let body = if self.loaded() {
            div()
                .flex()
                .flex_col()
                .when(self.snapshot.config_unreadable, |this| {
                    this.child(
                        div()
                            .mt(px(16.0))
                            .p(px(12.0))
                            .rounded(px(10.0))
                            .bg(p.drop_target())
                            .text_size(px(12.0))
                            .text_color(p.text())
                            .child(tr!(
                                "config.toml could not be read: settings apply but are not saved.",
                                "config.toml est illisible : les réglages s'appliquent sans s'enregistrer."
                            )),
                    )
                })
                .child(section_title(p, tr!("Voice", "Voix")))
                .child(self.voice_card(p, cx))
                .child(section_title(p, tr!("Display", "Affichage")))
                .child(self.display_card(p, window, cx))
                .child(section_title(p, tr!("Language", "Langue")))
                .child(self.language_card(p, cx))
                .child(
                    div()
                        .px(px(4.0))
                        .pt(px(14.0))
                        .text_size(px(12.0))
                        .text_color(p.muted())
                        .child(tr!(
                            "Every change is saved at once.",
                            "Chaque changement s'enregistre aussitôt."
                        )),
                )
                .into_any_element()
        } else {
            div()
                .pt(px(24.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .text_color(p.muted())
                .child(Icon::new(IconName::LoaderCircle).with_size(px(14.0)))
                .child(tr!("Reading the settings…", "Lecture des réglages…"))
                .into_any_element()
        };
        div()
            .id("settings-page")
            .debug_selector(|| "settings-page".to_string())
            .track_focus(&self.focus)
            .key_context("Settings")
            .on_key_down(cx.listener(Self::key_down))
            .flex()
            .flex_col()
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .bg(p.surface())
            .border_l_1()
            .border_color(p.border())
            .text_color(p.text())
            .child(self.header(p, cx))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px(px(16.0))
                    .pb(px(20.0))
                    .child(body),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .debug_selector(|| "settings-error".to_string())
                    .flex()
                    .flex_none()
                    .items_start()
                    .gap(px(6.0))
                    .px(px(20.0))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(p.border())
                    .text_size(px(12.0))
                    .text_color(p.error())
                    .child(Icon::new(IconName::TriangleAlert).with_size(px(14.0)))
                    .child(div().flex_1().child(error))
            }))
    }
}

//! La fenêtre principale : l'en-tête, la barre de recherche, le sommaire,
//! la vue de lecture, la barre de lecture, la barre d'état et l'aide, avec
//! la place des panneaux à venir (édition, réglages : tâches 8 et 9).
//!
//! Le focus est d'ordinaire sur la fenêtre (la vue de lecture) : les touches
//! y arrivent et passent par `keymap::action`. Le champ de recherche, quand
//! il a le focus, garde ses lettres (`n`, `t`, Espace s'y écrivent) ; seules
//! les commandes de la fenêtre (⌘W, ⌘O, ⌘F, ⌘,) passent. Le sommaire, quand
//! il a le focus, garde ↑ ↓ Entrée et Échap ; les autres touches du lecteur
//! y marchent. Ouvrir un fichier ou
//! lire le presse-papiers attend le lecteur sur l'exécuteur d'arrière-plan
//! de GPUI, jamais sur le fil de l'interface ; le résultat revient par
//! `update`, et une erreur reste dans la barre d'état jusqu'à la prochaine
//! ouverture réussie.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::view::{Snapshot, VoiceState};
use gpui::{
    div, prelude::*, px, App, ClipboardItem, Context, Entity, ExternalPaths, FocusHandle,
    FontWeight, IntoElement, KeyDownEvent, MouseButton, SharedString, Task, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Selectable, Sizable};
use gpui_kit_assets::IconName;

use crate::colors;
use crate::contents::{ContentsEvent, ContentsPanel};
use crate::help;
use crate::inputs::{self, command_modifier, key_press, open_path, paste_with, prompt_options};
use crate::keymap::{action, Action};
use crate::reading::ReadingView;
use crate::search::{SearchBar, SearchEvent};
use crate::session::Session;
use crate::transport::ReaderBar;

/// Le temps qu'un message passager (« Phrase copiée ») reste affiché.
const MESSAGE: Duration = Duration::from_secs(3);

/// Ce que dit la barre d'état, dans l'ordre de ce qui compte : l'erreur de
/// la dernière ouverture ou du dernier collage (le refus d'`OpenFile` revient
/// par sa réponse, pas par l'instantané), la voix qui s'installe ou ne va
/// pas, le message du lecteur, puis l'avis à lire. Vide quand tout va bien.
pub fn status_text(s: &Snapshot, error: Option<&str>) -> String {
    use dysflow::tr;
    let mut parts: Vec<String> = Vec::new();
    if let Some(error) = error {
        parts.push(error.to_string());
    }
    match &s.download {
        Some(d) => match &d.failed {
            Some(why) => parts.push(tr!(
                en = "The voice could not be installed: {}",
                fr = "La voix n'a pas pu s'installer : {}",
                why
            )),
            None => parts.push(tr!(
                en = "Installing the voice: {} of {} — the system voice reads in the meantime",
                fr = "Installation de la voix : {} sur {} — la voix du système lit en attendant",
                megabytes(d.received),
                megabytes(d.total)
            )),
        },
        None => match &s.voice {
            VoiceState::Unavailable { reason } => parts.push(tr!(
                en = "Voice unavailable: {}",
                fr = "Voix indisponible : {}",
                reason
            )),
            VoiceState::Fallback { reason } => parts.push(tr!(
                en = "System voice as a fallback: {}",
                fr = "Voix du système en repli : {}",
                reason
            )),
            VoiceState::Loading => {
                parts.push(tr!("Preparing the voice…", "Préparation de la voix…").to_string())
            }
            VoiceState::Resting | VoiceState::Ready => {}
        },
    }
    if !s.status.is_empty() {
        parts.push(s.status.clone());
    }
    if let Some(notice) = &s.notice {
        parts.push(notice.clone());
    }
    parts.join(" · ")
}

fn megabytes(bytes: u64) -> String {
    dysflow::tr!(en = "{:.0} MB", fr = "{:.0} Mo", bytes as f64 / 1_000_000.0)
}

/// Les panneaux posés à côté de la lecture. Échap ferme celui qui est
/// ouvert. La recherche n'en est pas un : sa barre s'ouvre en plus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Le sommaire.
    Contents,
    /// L'édition du texte (tâche 8).
    Edit,
    /// Les réglages (tâche 9).
    Settings,
}

type Clipboard = Arc<dyn Fn() -> Result<String, String> + Send + Sync>;

pub struct MainWindow {
    session: Arc<Session>,
    focus: FocusHandle,
    reading: Entity<ReadingView>,
    bar: Entity<ReaderBar>,
    search: Entity<SearchBar>,
    contents: Entity<ContentsPanel>,
    /// La barre de recherche est ouverte.
    searching: bool,
    /// Le presse-papiers lu par ⌘V : celui du système, sauf dans les essais.
    clipboard: Clipboard,
    help: bool,
    /// Le panneau ouvert : le sommaire (édition et réglages : tâches 8, 9).
    panel: Option<Panel>,
    /// La dernière erreur d'ouverture ou de collage.
    error: Option<String>,
    /// Ce qui s'ouvre en ce moment (« Ouverture de notes.pdf… »).
    opening: Option<String>,
    /// Un message passager : « Phrase copiée ».
    message: Option<SharedString>,
    title: String,
    renders: usize,
    _opening: Task<()>,
    _message: Task<()>,
    _observe: gpui::Subscription,
    _search: gpui::Subscription,
    _contents: gpui::Subscription,
}

impl MainWindow {
    /// La fenêtre principale, focalisée ; `initial` (le chemin donné en
    /// argument) s'ouvre aussitôt, sans attendre sur le fil de l'interface.
    pub fn build(
        session: Arc<Session>,
        initial: Option<PathBuf>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(session, initial, window, cx))
    }

    pub fn new(
        session: Arc<Session>,
        initial: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let reading = cx.new(|cx| ReadingView::new(Arc::clone(&session), cx));
        let bar = cx.new(|cx| ReaderBar::new(Arc::clone(&session), focus.clone(), window, cx));
        let search = cx.new(|cx| SearchBar::new(Arc::clone(&session), window, cx));
        let contents = cx.new(|cx| ContentsPanel::new(Arc::clone(&session), window, cx));
        let search_events = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            match event {
                SearchEvent::Submitted => {
                    // La lecture suit le résultat, les touches reviennent à
                    // la lecture (N, P, Espace).
                    this.reading.update(cx, |r, cx| r.follow(cx));
                    this.focus.focus(window, cx);
                }
                SearchEvent::Cancel => this.close_search(window, cx),
            }
        });
        let contents_events =
            cx.subscribe_in(
                &contents,
                window,
                |this, _, event, window, cx| match event {
                    ContentsEvent::Chosen(_) => {
                        this.reading.update(cx, |r, cx| r.follow(cx));
                        this.focus.focus(window, cx);
                    }
                    ContentsEvent::Close => this.close_contents(window, cx),
                },
            );
        // La vue de lecture ne prévient que si l'instantané a changé : la
        // fenêtre suit, sans boucle à elle.
        let observe = cx.observe_in(&reading, window, |this: &mut Self, reading, window, cx| {
            this.sync(&reading, window, cx)
        });
        let session_for_close = Arc::clone(&session);
        window.on_window_should_close(cx, move |_, _| {
            // Fermer la fenêtre arrête la lecture.
            session_for_close.send(Request::Command(Command::Pause));
            true
        });
        focus.focus(window, cx);
        let mut this = MainWindow {
            session,
            focus,
            reading,
            bar,
            search,
            contents,
            searching: false,
            clipboard: Arc::new(|| dysflow::source::clipboard().map_err(|e| format!("{e:#}"))),
            help: false,
            panel: None,
            error: None,
            opening: None,
            message: None,
            title: String::new(),
            renders: 0,
            _opening: Task::ready(()),
            _message: Task::ready(()),
            _observe: observe,
            _search: search_events,
            _contents: contents_events,
        };
        let reading = this.reading.clone();
        this.sync(&reading, window, cx);
        if let Some(path) = initial {
            this.open(path, cx);
        }
        this
    }

    /// Le focus des touches : celui de la vue de lecture.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    pub fn reading(&self) -> &Entity<ReadingView> {
        &self.reading
    }

    /// La dernière erreur d'ouverture ou de collage, effacée à la prochaine
    /// ouverture réussie.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn help_visible(&self) -> bool {
        self.help
    }

    /// Le panneau ouvert à côté de la lecture.
    pub fn panel(&self) -> Option<Panel> {
        self.panel
    }

    /// La barre de recherche est ouverte.
    pub fn search_visible(&self) -> bool {
        self.searching
    }

    pub fn search(&self) -> &Entity<SearchBar> {
        &self.search
    }

    pub fn contents(&self) -> &Entity<ContentsPanel> {
        &self.contents
    }

    /// Nombre de rendus de la fenêtre depuis sa création.
    pub fn renders(&self) -> usize {
        self.renders
    }

    /// Remplace le presse-papiers lu par ⌘V (les essais n'en lisent pas de
    /// vrai).
    pub fn set_clipboard(
        &mut self,
        read: impl Fn() -> Result<String, String> + Send + Sync + 'static,
    ) {
        self.clipboard = Arc::new(read);
    }

    /// La ligne de la barre d'état : le message passager, sinon ce qui
    /// s'ouvre, sinon `status_text`.
    pub fn status_line_for(&self, snapshot: &Snapshot) -> String {
        if let Some(message) = &self.message {
            return message.to_string();
        }
        let status = status_text(snapshot, self.error.as_deref());
        match &self.opening {
            Some(name) if status.is_empty() => name.clone(),
            Some(name) => format!("{name} · {status}"),
            None => status,
        }
    }

    pub fn status_line(&self) -> String {
        // Sans contexte, l'instantané du lecteur tel que la session le voit.
        self.status_line_for(&self.session.snapshot())
    }

    /// Ouvre un fichier (argument, boîte d'ouverture, dépose) sur
    /// l'exécuteur d'arrière-plan ; une ouverture en cours est remplacée.
    pub fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.opening = Some(dysflow::tr!(
            en = "Opening {}…",
            fr = "Ouverture de {}…",
            name
        ));
        cx.notify();
        let session = Arc::clone(&self.session);
        self._opening = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { open_path(&session, &path) })
                .await;
            let _ = this.update(cx, |this, cx| this.opened(result, cx));
        });
    }

    /// Lit le presse-papiers et l'ouvre, sur l'exécuteur d'arrière-plan.
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let session = Arc::clone(&self.session);
        let read = Arc::clone(&self.clipboard);
        self.opening =
            Some(dysflow::tr!("Reading the clipboard…", "Lecture du presse-papiers…").to_string());
        cx.notify();
        self._opening = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { paste_with(&session, || read()) })
                .await;
            let _ = this.update(cx, |this, cx| this.opened(result, cx));
        });
    }

    fn opened(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.opening = None;
        self.error = result.err();
        cx.notify();
    }

    /// La boîte d'ouverture du système ; le fichier choisi s'ouvre.
    pub fn prompt_open(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(prompt_options());
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| this.open(path, cx));
                }
            }
        })
        .detach();
    }

    fn say(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.message = Some(message.into());
        cx.notify();
        self._message = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(MESSAGE).await;
            let _ = this.update(cx, |this, cx| {
                this.message = None;
                cx.notify();
            });
        });
    }

    /// ⌘C : la sélection quand la vue de lecture en aura une, sinon la
    /// phrase lue.
    fn copy(&mut self, cx: &mut Context<Self>) {
        let reading = self.reading.read(cx);
        let sentence = reading.snapshot().sentence;
        let text = reading
            .document()
            .blocks
            .iter()
            .flat_map(|b| &b.sentences)
            .find(|s| s.id == sentence)
            .map(|s| {
                s.words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        match text {
            Some(text) if !text.is_empty() => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.say(dysflow::tr!("Sentence copied", "Phrase copiée"), cx);
            }
            _ => self.say(dysflow::tr!("Nothing to copy", "Rien à copier"), cx),
        }
    }

    /// Échap : l'aide, sinon la recherche, sinon le panneau ouvert, sinon
    /// la fenêtre (l'ordre de l'app Mac).
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.help {
            self.help = false;
            cx.notify();
        } else if self.searching {
            self.close_search(window, cx);
        } else if self.panel.is_some() {
            self.close_contents(window, cx);
        } else {
            self.close(window);
        }
    }

    /// / ou ⌘F : la barre s'ouvre (ou se retrouve) et le champ prend le
    /// focus, avec la requête déjà cherchée.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.reading.read(cx).snapshot().has_document {
            return;
        }
        self.searching = true;
        self.search.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// La recherche s'annule, la barre se ferme, la lecture reprend les
    /// touches.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.searching {
            self.search.update(cx, |s, cx| s.cancel(window, cx));
            self.searching = false;
            cx.notify();
        }
        self.focus.focus(window, cx);
    }

    /// t : le sommaire s'ouvre, la liste prend le focus sur la section lue ;
    /// ouvert, il se ferme.
    fn toggle_contents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Some(Panel::Contents) {
            self.close_contents(window, cx);
            return;
        }
        if !self.reading.read(cx).snapshot().has_document {
            return;
        }
        self.panel = Some(Panel::Contents);
        let reading = self.reading.clone();
        self.sync(&reading, window, cx);
        self.contents.update(cx, |c, cx| c.focus(window, cx));
        cx.notify();
    }

    fn close_contents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Some(Panel::Contents) {
            self.panel = None;
            cx.notify();
        }
        self.focus.focus(window, cx);
    }

    /// ⌘W : la lecture s'arrête, la fenêtre se ferme.
    fn close(&mut self, window: &mut Window) {
        self.session.send(Request::Command(Command::Pause));
        window.remove_window();
    }

    fn perform(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Reader(command) => {
                let navigates = !matches!(command, Command::ChangeSpeed(_));
                self.session.send(Request::Command(command));
                if navigates {
                    // Une commande de lecture ramène la vue sur la ligne lue.
                    self.reading.update(cx, |r, cx| r.follow(cx));
                }
            }
            Action::Line(delta) => self.reading.update(cx, |r, cx| r.line(delta, cx)),
            Action::Follow => self.reading.update(cx, |r, cx| r.follow(cx)),
            Action::Escape => self.escape(window, cx),
            Action::Close => self.close(window),
            Action::Copy => self.copy(cx),
            Action::Paste => self.paste(cx),
            Action::OpenFile => self.prompt_open(cx),
            Action::ShowHelp => {
                self.help = !self.help;
                cx.notify();
            }
            Action::OpenSearch => self.open_search(window, cx),
            Action::ToggleContents => self.toggle_contents(window, cx),
            // L'édition et les réglages arrivent avec les tâches 8 et 9.
            Action::Edit | Action::OpenSettings => {}
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let focused = window.focused(cx);
        let in_search = self.searching && self.search.read(cx).focus_handle(cx).is_focused(window);
        let in_contents = self.panel == Some(Panel::Contents)
            && self.contents.read(cx).focus_handle(cx).is_focused(window);
        // Un menu ouvert (vitesse) garde ses touches.
        if !in_search && !in_contents && focused.is_some_and(|f| f != self.focus) {
            return;
        }
        let Some(found) = action(&key_press(&event.keystroke), command_modifier()) else {
            return;
        };
        if in_search {
            // Le champ garde ses lettres, Espace, ses flèches, ⌘C, ⌘V ;
            // Échap y est pris par la barre. Seules les commandes de la
            // fenêtre passent.
            if !matches!(
                found,
                Action::Close | Action::OpenFile | Action::OpenSearch | Action::OpenSettings
            ) {
                return;
            }
        } else if in_contents && matches!(found, Action::Escape | Action::Line(_)) {
            // La liste a déjà pris Échap (elle ferme le sommaire) et ↑ ↓.
            return;
        }
        // Une touche tenue répète la navigation, pas l'ouverture ni la
        // fermeture.
        let repeats = matches!(found, Action::Reader(_) | Action::Line(_));
        if event.is_held && !repeats {
            cx.stop_propagation();
            return;
        }
        cx.stop_propagation();
        self.perform(found, window, cx);
    }

    /// Le nouvel état publié : la barre, le titre de la fenêtre.
    fn sync(&mut self, reading: &Entity<ReadingView>, window: &mut Window, cx: &mut Context<Self>) {
        let (snapshot, count, title) = {
            let r = reading.read(cx);
            (
                r.snapshot().clone(),
                r.document().sentence_count,
                r.document().title.clone(),
            )
        };
        self.bar
            .update(cx, |bar, cx| bar.receive(&snapshot, count, window, cx));
        self.search.update(cx, |s, cx| s.receive(&snapshot, cx));
        if self.panel == Some(Panel::Contents) {
            let document = Arc::clone(reading.read(cx).document());
            self.contents.update(cx, |c, cx| {
                c.receive(&document, snapshot.section, window, cx)
            });
        }
        if title != self.title {
            let shown = if title.is_empty() {
                "Dysflow".to_string()
            } else {
                format!("{title} — Dysflow")
            };
            window.set_window_title(&shown);
            self.title = title;
        }
        cx.notify();
    }

    fn header(&self, has_document: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if has_document && !self.title.is_empty() {
            self.title.clone()
        } else {
            "Dysflow".to_string()
        };
        let button = |id: &'static str, icon: IconName, label: &'static str, tip: String| {
            Button::new(id)
                .ghost()
                .icon(icon)
                .label(label)
                .with_size(gpui_component::Size::Medium)
                .h(px(32.0))
                .rounded(px(10.0))
                .tab_stop(false)
                .tooltip(tip)
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(48.0))
            .px(px(16.0))
            .border_b_1()
            .border_color(colors::border())
            .text_size(px(13.0))
            .text_color(colors::chrome())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::text())
                    .child(title),
            )
            .child(
                button(
                    "open",
                    IconName::FolderOpen,
                    dysflow::tr!("Open", "Ouvrir"),
                    format!(
                        "{} ({})",
                        dysflow::tr!("Open a file", "Ouvrir un fichier"),
                        inputs::command_label("o")
                    ),
                )
                .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx))),
            )
            .child(
                button(
                    "paste",
                    IconName::ClipboardPaste,
                    dysflow::tr!("Paste", "Coller"),
                    format!(
                        "{} ({})",
                        dysflow::tr!("Read the clipboard", "Lire le presse-papiers"),
                        inputs::command_label("v")
                    ),
                )
                .on_click(cx.listener(|this, _, _, cx| this.paste(cx))),
            )
            .child(
                div()
                    .debug_selector(|| "header-contents".to_string())
                    .child(
                        Button::new("header-contents")
                            .ghost()
                            .icon(IconName::TableOfContents)
                            .label(dysflow::tr!("Contents", "Sommaire"))
                            .with_size(gpui_component::Size::Medium)
                            .h(px(32.0))
                            .rounded(px(10.0))
                            .tab_stop(false)
                            .selected(self.panel == Some(Panel::Contents))
                            .disabled(!has_document)
                            .tooltip(format!("{} (t)", dysflow::tr!("Contents", "Sommaire")))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_contents(window, cx)),
                            ),
                    ),
            )
            .child(
                div().debug_selector(|| "header-search".to_string()).child(
                    Button::new("header-search")
                        .ghost()
                        .icon(IconName::Search)
                        .label(dysflow::tr!("Search", "Rechercher"))
                        .with_size(gpui_component::Size::Medium)
                        .h(px(32.0))
                        .rounded(px(10.0))
                        .tab_stop(false)
                        .selected(self.searching)
                        .disabled(!has_document)
                        .tooltip(format!(
                            "{} (/ {} {})",
                            dysflow::tr!("Search", "Rechercher"),
                            dysflow::tr!("or", "ou"),
                            inputs::command_label("f")
                        ))
                        .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
                ),
            )
            .child(
                Button::new("help")
                    .ghost()
                    .icon(IconName::CircleQuestionMark)
                    .with_size(px(16.0))
                    .size(px(32.0))
                    .rounded(px(10.0))
                    .tab_stop(false)
                    .selected(self.help)
                    .tooltip(dysflow::tr!("Shortcuts (?)", "Raccourcis (?)"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.help = !this.help;
                        cx.notify();
                    })),
            )
    }

    /// Sans document : l'invitation, comme le lecteur vide de l'app Mac.
    fn invitation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(14.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::text())
                    .child(dysflow::tr!("Nothing to read", "Rien à lire")),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(colors::muted())
                    .child(dysflow::tr!(
                        en = "Paste some text ({}) or drop in a file",
                        fr = "Colle un texte ({}) ou dépose un fichier",
                        inputs::command_label("v")
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        Button::new("invitation-open")
                            .outline()
                            .icon(IconName::FolderOpen)
                            .label(dysflow::tr!("Open a file", "Ouvrir un fichier"))
                            .tab_stop(false)
                            .on_click(cx.listener(|this, _, _, cx| this.prompt_open(cx))),
                    )
                    .child(
                        Button::new("invitation-paste")
                            .outline()
                            .icon(IconName::ClipboardPaste)
                            .label(dysflow::tr!("Paste", "Coller"))
                            .tab_stop(false)
                            .on_click(cx.listener(|this, _, _, cx| this.paste(cx))),
                    ),
            )
    }

    fn status_bar(&self, snapshot: &Snapshot) -> impl IntoElement {
        let line = self.status_line_for(snapshot);
        let failing = self.error.is_some() && self.message.is_none();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(16.0))
            .text_size(px(12.0))
            .text_color(if failing {
                colors::error()
            } else {
                colors::muted()
            })
            .when(failing, |this| {
                this.child(
                    gpui_component::Icon::new(IconName::TriangleAlert)
                        .with_size(px(14.0))
                        .text_color(colors::error()),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(line),
            )
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let snapshot = self.reading.read(cx).snapshot().clone();
        let has_document = snapshot.has_document;
        let reading = div()
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            // Un clic dans la page rend les touches à la lecture.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.focus.focus(window, cx)),
            )
            .child(self.reading.clone())
            .when(!has_document && self.opening.is_none(), |this| {
                this.child(self.invitation(cx))
            })
            .when(self.help, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(16.0))
                        .bg(colors::overlay())
                        .child(help::card(command_modifier())),
                )
            });
        div()
            .id("fenetre")
            .track_focus(&self.focus)
            .key_context("Dysflow")
            .on_key_down(cx.listener(Self::key_down))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                // Un seul document à la fois : le premier fichier déposé.
                if let Some(path) = paths.paths().first() {
                    this.open(path.clone(), cx);
                }
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(colors::drop_target()))
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::background())
            .text_color(colors::text())
            .child(self.header(has_document, cx))
            .when(self.searching, |this| this.child(self.search.clone()))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .when(self.panel == Some(Panel::Contents), |this| {
                        this.child(self.contents.clone())
                    })
                    .child(reading),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .pt(px(8.0))
                    // Un clic à côté des boutons rend aussi les touches à la
                    // lecture.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.focus.focus(window, cx)),
                    )
                    .child(self.bar.clone()),
            )
            .child(self.status_bar(&snapshot))
    }
}

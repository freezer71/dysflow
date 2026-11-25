//! La fenêtre principale : l'en-tête, la barre de recherche, le sommaire,
//! la vue de lecture (ou, à sa place, l'éditeur du texte), les réglages à
//! droite, la barre de lecture, la barre d'état et l'aide.
//!
//! Le focus est d'ordinaire sur la fenêtre (la vue de lecture) : les touches
//! y arrivent et passent par `keymap::action`. Le champ de recherche, quand
//! il a le focus, garde ses lettres (`n`, `t`, Espace s'y écrivent) ; seules
//! les commandes de la fenêtre (⌘W, ⌘O, ⌘F, ⌘,) passent. Le sommaire, quand
//! il a le focus, garde ↑ ↓ Entrée et Échap ; les autres touches du lecteur
//! y marchent. L'éditeur, quand il a le focus, garde ses touches comme le
//! champ de recherche (Entrée y va à la ligne) ; Échap y valide. Ouvrir un
//! fichier ou lire le presse-papiers attend le lecteur sur l'exécuteur d'arrière-plan
//! de GPUI, jamais sur le fil de l'interface ; le résultat revient par
//! `update`, et une erreur reste dans la barre d'état jusqu'à la prochaine
//! ouverture réussie. Les réglages, quand ils ont le focus, gardent leurs
//! touches (Espace y presse un bouton) ; Échap les ferme, ⌘, aussi.
//!
//! La fenêtre suit l'apparence du système (`observe_window_appearance`) ;
//! le thème choisi dans les réglages, la langue de l'interface et les
//! réglages de lecture s'appliquent dès qu'ils sont enregistrés.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::view::Snapshot;
use gpui::{
    div, prelude::*, px, App, ClipboardItem, Context, Entity, ExternalPaths, FocusHandle,
    FontWeight, IntoElement, KeyDownEvent, MouseButton, SharedString, Task, Window,
    WindowAppearance,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Selectable, Sizable};
use gpui_kit_assets::IconName;

use crate::contents::{ContentsEvent, ContentsPanel};
use crate::editor::{ask_draft, EditorEvent, Outcome, TextEditor};
use crate::help;
use crate::inputs::{self, command_modifier, key_press, open_path, paste_with, prompt_options};
use crate::keymap::{action, Action, Modifier};
use crate::reading::ReadingView;
use crate::search::{SearchBar, SearchEvent};
use crate::session::{Reply, Session};
use crate::settings::{SettingsEvent, SettingsPage};
use crate::startup::{first_launch, retry_label, VoiceStatus};
use crate::theme;
use crate::transport::{BarEvent, ReaderBar};

/// Le temps qu'un message passager (« Phrase copiée ») reste affiché.
const MESSAGE: Duration = Duration::from_secs(3);

/// Ce que dit la barre d'état, dans l'ordre de ce qui compte : l'erreur de
/// la dernière ouverture ou du dernier collage (le refus d'`OpenFile` revient
/// par sa réponse, pas par l'instantané), la voix qui s'installe ou ne va
/// pas (`startup::VoiceStatus`), le message du lecteur, puis l'avis à lire.
/// Vide quand tout va bien.
pub fn status_text(s: &Snapshot, error: Option<&str>) -> String {
    compose(error, VoiceStatus::current(s, None).map(|v| v.text()), s)
}

fn compose(error: Option<&str>, voice: Option<String>, s: &Snapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(error) = error {
        parts.push(error.to_string());
    }
    parts.extend(voice);
    if !s.status.is_empty() {
        parts.push(s.status.clone());
    }
    if let Some(notice) = &s.notice {
        parts.push(notice.clone());
    }
    parts.join(" · ")
}

/// Les panneaux posés à côté de la lecture. Échap ferme celui qui est
/// ouvert. La recherche n'en est pas un : sa barre s'ouvre en plus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Le sommaire.
    Contents,
    /// L'édition du texte : l'éditeur prend la place de la lecture.
    Edit,
    /// Les réglages, à droite de la lecture.
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
    editor: Entity<TextEditor>,
    settings: Entity<SettingsPage>,
    /// La barre de recherche est ouverte.
    searching: bool,
    /// Le presse-papiers lu par ⌘V : celui du système, sauf dans les essais.
    clipboard: Clipboard,
    help: bool,
    /// Le panneau ouvert : le sommaire, l'éditeur ou les réglages.
    panel: Option<Panel>,
    /// La dernière erreur d'ouverture ou de collage.
    error: Option<String>,
    /// Ce qui s'ouvre en ce moment (« Ouverture de notes.pdf… »).
    opening: Option<String>,
    /// Un message passager : « Phrase copiée ».
    message: Option<SharedString>,
    title: String,
    /// La lecture reprend le focus au prochain passage (l'éditeur fermé
    /// sans fenêtre à portée).
    refocus: bool,
    renders: usize,
    _opening: Task<()>,
    _message: Task<()>,
    _observe: gpui::Subscription,
    _search: gpui::Subscription,
    _contents: gpui::Subscription,
    _editor: gpui::Subscription,
    _settings: gpui::Subscription,
    _bar: gpui::Subscription,
    _appearance: gpui::Subscription,
    /// La question au lecteur qui ouvre l'éditeur.
    _editing: Task<()>,
    /// Cette question est en route : un second `e` n'en pose pas d'autre
    /// (il perdrait que la voix lisait).
    edit_pending: bool,
    /// Le refus de la dernière demande d'installation de la voix
    /// (`first_launch`), jusqu'au prochain « Réessayer ».
    voice_setup: Option<String>,
    /// L'attente de cette réponse, sur l'exécuteur d'arrière-plan.
    _voice_setup: Task<()>,
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
        let editor = cx.new(|cx| TextEditor::new(Arc::clone(&session), window, cx));
        let settings = cx.new(|cx| SettingsPage::new(Arc::clone(&session), window, cx));
        let settings_events =
            cx.subscribe_in(
                &settings,
                window,
                |this, _, event, window, cx| match event {
                    SettingsEvent::Saved(values) => this.settings_saved(values, window, cx),
                    SettingsEvent::Close => this.close_settings(window, cx),
                },
            );
        let bar_events = cx.subscribe_in(&bar, window, |this, _, event, window, cx| match event {
            BarEvent::Settings => this.toggle_settings(window, cx),
        });
        // Le thème : posé au démarrage (`theme::install`), sinon ici ; puis
        // l'apparence du système, et chacun de ses changements.
        crate::theme::ensure(cx);
        crate::theme::set_system_appearance(window.appearance(), cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            this.system_appearance_changed(window.appearance(), window, cx)
        });
        let editor_events =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                EditorEvent::Done => this.finish_editing(true, window, cx),
                EditorEvent::Cancel => this.cancel_editing(window, cx),
            });
        let search_events = cx.subscribe_in(&search, window, |this, _, event, window, cx| {
            match event {
                SearchEvent::Submitted => {
                    // La lecture suit le résultat, les touches reviennent à
                    // la lecture (N, P, Espace).
                    this.reading.update(cx, |r, cx| r.follow(cx));
                    this.focus.focus(window, cx);
                }
                SearchEvent::Moved => this.reading.update(cx, |r, cx| r.follow(cx)),
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
            editor,
            settings,
            searching: false,
            clipboard: Arc::new(|| dysflow::source::clipboard().map_err(|e| format!("{e:#}"))),
            help: false,
            panel: None,
            error: None,
            opening: None,
            message: None,
            title: String::new(),
            refocus: false,
            renders: 0,
            _opening: Task::ready(()),
            _message: Task::ready(()),
            _observe: observe,
            _search: search_events,
            _contents: contents_events,
            _editor: editor_events,
            _settings: settings_events,
            _bar: bar_events,
            _appearance: appearance,
            _editing: Task::ready(()),
            edit_pending: false,
            voice_setup: None,
            _voice_setup: Task::ready(()),
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

    /// L'éditeur du texte est ouvert.
    pub fn editing(&self) -> bool {
        self.panel == Some(Panel::Edit)
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

    pub fn editor(&self) -> &Entity<TextEditor> {
        &self.editor
    }

    pub fn settings(&self) -> &Entity<SettingsPage> {
        &self.settings
    }

    /// L'apparence du système a changé : sous le thème « Système », la
    /// fenêtre passe en clair ou en sombre (appelée par l'observateur de
    /// GPUI ; les essais l'appellent eux-mêmes, la plateforme de test ne
    /// sachant pas simuler ce changement).
    pub fn system_appearance_changed(
        &mut self,
        appearance: WindowAppearance,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::theme::set_system_appearance(appearance, cx);
        cx.notify();
    }

    /// ⌘, ou l'écrou : les réglages s'ouvrent à droite de la lecture, avec
    /// le clavier ; leurs valeurs se lisent sans bloquer.
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing() {
            self.finish_editing(true, window, cx);
        }
        self.help = false;
        self.panel = Some(Panel::Settings);
        let snapshot = self.reading.read(cx).snapshot().clone();
        self.settings.update(cx, |page, cx| {
            page.receive(&snapshot, cx);
            page.load(cx);
            page.focus(window, cx);
        });
        self.bar
            .update(cx, |bar, cx| bar.set_settings_open(true, cx));
        cx.notify();
    }

    /// Les réglages se ferment ; la lecture reprend les touches.
    pub fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Some(Panel::Settings) {
            self.panel = None;
            self.bar
                .update(cx, |bar, cx| bar.set_settings_open(false, cx));
            cx.notify();
        }
        self.focus.focus(window, cx);
    }

    fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel == Some(Panel::Settings) {
            self.close_settings(window, cx);
        } else {
            self.open_settings(window, cx);
        }
    }

    /// Un réglage est enregistré : la vue de lecture relit les siens
    /// (police, taille, règle…), le thème et la langue de l'interface
    /// suivent, et toute la fenêtre se redessine.
    fn settings_saved(
        &mut self,
        values: &dysflow::reader::settings::SettingsView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading.update(cx, |r, cx| r.reload_settings(cx));
        crate::theme::set_choice(values.theme, cx);
        let lang = crate::settings::ui_lang_for(&values.ui_language);
        if lang != dysflow::i18n::ui_lang() {
            dysflow::i18n::set_ui_lang(lang);
            // Les textes gardés par les composants (le champ de recherche)
            // se réécrivent ; le reste se relit au prochain rendu.
            self.search.update(cx, |s, cx| s.relabel(window, cx));
            cx.refresh_windows();
        }
        cx.notify();
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

    /// Attend la réponse de `first_launch` sur l'exécuteur d'arrière-plan,
    /// jamais sur le fil de l'interface ; un refus reste dans la barre
    /// d'état, avec « Réessayer ».
    pub fn watch_voice_setup(&mut self, reply: Reply<Result<(), String>>, cx: &mut Context<Self>) {
        self._voice_setup = cx.spawn(async move |this, cx| {
            let answer = cx
                .background_executor()
                .spawn(async move { reply.wait_unbounded() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.voice_setup = match answer {
                    Some(Ok(())) => None,
                    Some(Err(why)) => Some(why),
                    None => Some(
                        dysflow::tr!("the reader has stopped", "le lecteur s'est arrêté")
                            .to_string(),
                    ),
                };
                cx.notify();
            });
        });
    }

    /// « Réessayer » : redemande l'installation de la voix au cœur.
    pub fn retry_voice(&mut self, cx: &mut Context<Self>) {
        self.voice_setup = None;
        let reply = first_launch(&self.session);
        self.watch_voice_setup(reply, cx);
        cx.notify();
    }

    /// L'installation de la voix a échoué : la barre d'état offre
    /// « Réessayer ».
    pub fn can_retry_voice(&self) -> bool {
        self.voice_status(&self.session.snapshot())
            .is_some_and(|v| v.can_retry())
    }

    fn voice_status(&self, snapshot: &Snapshot) -> Option<VoiceStatus> {
        VoiceStatus::current(snapshot, self.voice_setup.as_deref())
    }

    /// La ligne de la barre d'état : le message passager, sinon ce qui
    /// s'ouvre, sinon `status_text` (avec le refus de l'installation de la
    /// voix).
    pub fn status_line_for(&self, snapshot: &Snapshot) -> String {
        self.status_line_with(snapshot, |v| v.text())
    }

    /// `status_line_for`, la voix dite par `voice` (la barre ôte
    /// « Réessayer », qu'elle montre en bouton).
    fn status_line_with(
        &self,
        snapshot: &Snapshot,
        voice: impl Fn(&VoiceStatus) -> String,
    ) -> String {
        if let Some(message) = &self.message {
            return message.to_string();
        }
        let status = compose(
            self.error.as_deref(),
            self.voice_status(snapshot).map(|v| voice(&v)),
            snapshot,
        );
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
        // Une modification en cours s'applique d'abord : la file du lecteur
        // l'ouvre avant le fichier.
        self.apply_editing(cx);
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
        self.apply_editing(cx);
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

    /// Échap : la saisie se valide, sinon l'aide, la recherche, le panneau
    /// ouvert (l'ordre de l'app Mac). Rien d'ouvert : Échap ne fait rien ;
    /// seul ⌘W (Ctrl+W ailleurs) ferme la fenêtre.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing() {
            self.finish_editing(true, window, cx);
        } else if self.help {
            self.help = false;
            cx.notify();
        } else if self.searching {
            self.close_search(window, cx);
        } else if self.panel == Some(Panel::Settings) {
            self.close_settings(window, cx);
        } else if self.panel.is_some() {
            self.close_contents(window, cx);
        }
    }

    /// / ou ⌘F : la barre s'ouvre (ou se retrouve) et le champ prend le
    /// focus, avec la requête déjà cherchée.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing() {
            self.finish_editing(true, window, cx);
        }
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
        if self.editing() {
            self.finish_editing(true, window, cx);
        }
        if self.panel == Some(Panel::Contents) {
            self.close_contents(window, cx);
            return;
        }
        if !self.reading.read(cx).snapshot().has_document {
            return;
        }
        if self.panel == Some(Panel::Settings) {
            self.bar
                .update(cx, |bar, cx| bar.set_settings_open(false, cx));
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

    /// e ou le bouton « Modifier » : la lecture se met en pause, le texte
    /// ouvert est demandé au lecteur (sur l'exécuteur d'arrière-plan), puis
    /// l'éditeur s'ouvre dessus avec le focus. La recherche et l'aide se
    /// ferment, comme dans l'app Mac.
    pub fn begin_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.reading.read(cx).snapshot().clone();
        if self.editing() || self.edit_pending || !snapshot.has_document {
            return;
        }
        // L'instantané publié, plus frais que celui de la vue (sondée moins
        // souvent à l'arrêt). La pause part toujours : le lecteur l'ignore
        // sans lecture en cours.
        let resume = self.session.snapshot().playing;
        self.session.send(Request::Command(Command::Pause));
        if self.searching {
            self.close_search(window, cx);
        }
        self.help = false;
        let font = self.reading.read(cx).editor_font();
        let session = Arc::clone(&self.session);
        let revision = snapshot.revision;
        self.edit_pending = true;
        self._editing = cx.spawn_in(window, async move |this, cx| {
            let draft = cx
                .background_executor()
                .spawn(async move { ask_draft(&session) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.edit_pending = false;
                let current = this.reading.read(cx).snapshot().clone();
                // Sans texte, ou un autre document arrivé entre-temps : rien
                // ne s'ouvre ; la voix reprend si elle lisait.
                let Some(draft) = draft.filter(|_| current.revision == revision) else {
                    if resume {
                        this.session.send(Request::Command(Command::TogglePlay));
                    }
                    return;
                };
                this.panel = Some(Panel::Edit);
                this.editor.update(cx, |e, cx| {
                    e.begin(draft, revision, resume, font, window, cx)
                });
                cx.notify();
            });
        });
        cx.notify();
    }

    /// Échap, ⌘↩, « Terminé » : la modification s'applique, l'éditeur se
    /// ferme, la lecture reprend les touches. `resume` faux : la voix reste
    /// en pause.
    fn finish_editing(&mut self, resume: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing() {
            return;
        }
        let sentence = self.session.snapshot().sentence;
        let outcome = self
            .editor
            .update(cx, |e, cx| e.finish(sentence, resume, cx));
        self.panel = None;
        if outcome == Outcome::Reopened {
            self.say(dysflow::tr!("Text edited", "Texte modifié"), cx);
        }
        self.reading.update(cx, |r, cx| r.follow(cx));
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// La modification s'applique sans fenêtre à portée (ouverture d'un
    /// fichier, collage) ; le focus revient à la lecture au prochain rendu.
    fn apply_editing(&mut self, cx: &mut Context<Self>) {
        if !self.editing() {
            return;
        }
        let sentence = self.session.snapshot().sentence;
        self.editor
            .update(cx, |e, cx| e.finish(sentence, false, cx));
        self.panel = None;
        self.refocus = true;
        cx.notify();
    }

    /// « Annuler » : la modification s'oublie, la voix repart si elle lisait.
    fn cancel_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing() {
            return;
        }
        self.editor.update(cx, |e, _| e.cancel());
        self.panel = None;
        self.focus.focus(window, cx);
        cx.notify();
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
            Action::Edit => self.begin_editing(window, cx),
            Action::OpenSettings => self.toggle_settings(window, cx),
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let focused = window.focused(cx);
        let in_search = self.searching && self.search.read(cx).focus_handle(cx).is_focused(window);
        let in_contents = self.panel == Some(Panel::Contents)
            && self.contents.read(cx).focus_handle(cx).is_focused(window);
        let in_editor = self.editing() && self.editor.read(cx).focus_handle(cx).is_focused(window);
        // Les réglages gardent leurs touches (Espace presse un bouton, ← →
        // déplacent un curseur), comme un champ ; Échap y est pris par la
        // page.
        let in_settings = self.panel == Some(Panel::Settings)
            && self
                .settings
                .read(cx)
                .focus_handle()
                .contains_focused(window, cx);
        // Un menu ouvert (vitesse) garde ses touches.
        if !in_search
            && !in_contents
            && !in_editor
            && !in_settings
            && focused.is_some_and(|f| f != self.focus)
        {
            return;
        }
        let Some(found) = action(&key_press(&event.keystroke), command_modifier()) else {
            return;
        };
        if in_search || in_editor || in_settings {
            // Le champ garde ses lettres, ses signes (/ ? + -), Espace, ses
            // flèches, ⌘C, ⌘V ; Échap y est pris par la barre (ou par
            // l'éditeur, qui valide), Entrée va à la ligne. Seules les
            // commandes de la fenêtre passent, et seulement avec la touche
            // de commande (/ sans elle s'écrit : « et/ou »).
            let m = event.keystroke.modifiers;
            let with_command = match command_modifier() {
                Modifier::Command => m.platform,
                Modifier::Control => m.control,
            };
            if !with_command
                || !matches!(
                    found,
                    Action::Close | Action::OpenFile | Action::OpenSearch | Action::OpenSettings
                )
            {
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
        if self.panel == Some(Panel::Settings) {
            self.settings
                .update(cx, |page, cx| page.receive(&snapshot, cx));
        }
        if self.editing() && self.editor.read(cx).revision() != snapshot.revision {
            // Un autre document est arrivé : le texte modifié ne vaut plus,
            // il s'oublie (rien ne part au lecteur).
            self.panel = None;
            self.focus.focus(window, cx);
        }
        if self.refocus && !self.editing() {
            self.refocus = false;
            self.focus.focus(window, cx);
        }
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
        let p = theme::colors(cx);
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
            .border_color(p.border())
            .text_size(px(13.0))
            .text_color(p.chrome())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text())
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
                div().debug_selector(|| "header-edit".to_string()).child(
                    Button::new("header-edit")
                        .ghost()
                        .icon(IconName::Pencil)
                        .with_size(px(16.0))
                        .size(px(32.0))
                        .rounded(px(10.0))
                        .tab_stop(false)
                        .selected(self.editing())
                        .disabled(!has_document)
                        .tooltip(format!(
                            "{} (e)",
                            dysflow::tr!("Edit the text", "Modifier le texte")
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.editing() {
                                this.finish_editing(true, window, cx);
                            } else {
                                this.begin_editing(window, cx);
                            }
                        })),
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
        let p = theme::colors(cx);
        div()
            .debug_selector(|| "invitation".to_string())
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
                    .text_color(p.text())
                    .child(dysflow::tr!("Nothing to read", "Rien à lire")),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(p.muted())
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

    fn status_bar(
        &self,
        snapshot: &Snapshot,
        p: &theme::Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let voice = self
            .voice_status(snapshot)
            .filter(|_| self.message.is_none());
        let retry = voice.as_ref().is_some_and(|v| v.can_retry());
        let progress = voice.as_ref().and_then(|v| v.fraction());
        let line = self.status_line_with(snapshot, |v| v.message());
        let failing = (self.error.is_some() || retry) && self.message.is_none();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(28.0))
            .px(px(16.0))
            .text_size(px(12.0))
            .text_color(if failing { p.error() } else { p.muted() })
            .when(failing, |this| {
                this.child(
                    gpui_component::Icon::new(IconName::TriangleAlert)
                        .with_size(px(14.0))
                        .text_color(p.error()),
                )
            })
            .when_some(progress, |this, fraction| {
                // L'avancement de l'installation, comme la jauge de l'app
                // Mac : une piste au filet, remplie à l'accent.
                this.child(
                    div()
                        .debug_selector(|| "voice-progress".to_string())
                        .flex_none()
                        .w(px(80.0))
                        .h(px(4.0))
                        .rounded_full()
                        .bg(p.border())
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .bg(p.accent())
                                .w(px(80.0 * fraction.clamp(0.0, 1.0))),
                        ),
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
            .when(retry, |this| {
                this.child(
                    div().debug_selector(|| "voice-retry".to_string()).child(
                        Button::new("voice-retry")
                            .ghost()
                            .xsmall()
                            .label(retry_label())
                            .on_click(cx.listener(|this, _, _, cx| this.retry_voice(cx))),
                    ),
                )
            })
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
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
                        .bg(p.overlay())
                        .child(help::card(command_modifier(), p)),
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
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(p.drop_target()))
            .size_full()
            .flex()
            .flex_col()
            .bg(p.background())
            .text_color(p.text())
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
                    .when(self.editing(), |this| this.child(self.editor.clone()))
                    .when(!self.editing(), |this| this.child(reading))
                    .when(self.panel == Some(Panel::Settings), |this| {
                        this.child(self.settings.clone())
                    }),
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
            .child(self.status_bar(&snapshot, p, cx))
    }
}

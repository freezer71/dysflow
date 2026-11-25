//! L'édition du texte lu (e) : le champ multiligne de gpui-component, à la
//! police et à la taille de lecture, et sous lui la barre « Modification du
//! texte » (Échap valide, Annuler, Terminé).
//!
//! Comme l'app Mac (`PanelController.beginEditing` / `finishEditing`) :
//! ouvrir l'éditeur met la lecture en pause ; Échap, ⌘↩ ou « Terminé »
//! valident : le texte modifié remplace le document, sous la même source,
//! et la voix reprend à la même phrase (la dernière si le texte a raccourci),
//! puis repart si elle lisait. Un texte inchangé ne recharge rien.
//! « Annuler » oublie les modifications. Écart voulu : un texte vidé ferme
//! le document (« Rien à lire ») au lieu d'être ignoré.
//!
//! Le texte à modifier vient du fil du lecteur (`editable_markdown`) par
//! une question posée sur l'exécuteur d'arrière-plan (voir
//! `MainWindow::begin_editing`) ; la validation n'attend rien : elle met en
//! file l'ouverture, puis la reprise.

use std::sync::Arc;
use std::time::Instant;

use dysflow::reader::command::Command;
use dysflow::reader::editable::{editable_markdown, editable_source, resume_sentence};
use dysflow::reader::handle::Request;
use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, SharedString, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Enter, Escape, InputEvent, Textarea, TextareaState};
use gpui_component::{Icon, Sizable};
use gpui_kit_assets::IconName;

use crate::session::Session;
use crate::theme;

/// Le texte à modifier et la source sous laquelle il se rouvrira.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub markdown: String,
    pub source: String,
}

/// Demande au fil du lecteur le texte à modifier. BLOQUE jusqu'à sa réponse
/// (5 s au plus) : jamais depuis le fil de l'interface. `None` sans document
/// ou sans réponse.
pub fn ask_draft(session: &Session) -> Option<Draft> {
    session
        .ask(|reader| {
            let markdown = editable_markdown(reader)?;
            let source = editable_source(reader).unwrap_or_else(|| "edit".to_string());
            Some(Draft { markdown, source })
        })
        .flatten()
}

/// Ce qu'a fait la validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Rien n'a changé : rien ne se recharge.
    Unchanged,
    /// Le texte vidé : plus rien à lire.
    Cleared,
    /// Le texte modifié remplace le document.
    Reopened,
}

/// Applique une modification : `draft` remplace `origin` sous `source`.
/// Rien ne bloque : l'ouverture part dans la file du lecteur, puis, dans la
/// même file (elle s'applique donc au nouveau document), la reprise à
/// `resume_sentence(previous, phrases du nouveau texte)` et la voix remise
/// en route si `play`.
pub fn apply(
    session: &Session,
    origin: &str,
    draft: &str,
    source: &str,
    previous: usize,
    play: bool,
) -> Outcome {
    if draft == origin {
        if play {
            session.send(Request::Command(Command::TogglePlay));
        }
        return Outcome::Unchanged;
    }
    if draft.trim().is_empty() {
        session.send(Request::Clear);
        return Outcome::Cleared;
    }
    session.send(Request::OpenText {
        markdown: draft.to_string(),
        source: source.to_string(),
    });
    session.send(Request::Ask(Box::new(move |reader| {
        let count = reader.session().map_or(0, |app| app.doc().sentences.len());
        let now = Instant::now();
        if let Some(sentence) = resume_sentence(previous, count) {
            reader.apply(Command::SeekTo(sentence), now);
        }
        let playing = dysflow::reader::view::snapshot(reader).playing;
        if play != playing {
            let command = if play {
                Command::TogglePlay
            } else {
                Command::Pause
            };
            reader.apply(command, now);
        }
    })));
    Outcome::Reopened
}

/// Ce que l'éditeur demande à la fenêtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEvent {
    /// Échap, ⌘↩ ou « Terminé » : la modification s'applique.
    Done,
    /// « Annuler » : la modification s'oublie.
    Cancel,
}

/// La police de lecture, reprise dans le champ.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorFont {
    pub family: SharedString,
    pub size: f32,
    pub line_height: f32,
}

pub struct TextEditor {
    session: Arc<Session>,
    input: Entity<TextareaState>,
    /// Le texte tel qu'il était à l'ouverture de l'éditeur.
    origin: String,
    source: String,
    /// La révision du document modifié : un autre document arrivé entre-temps
    /// rend la modification caduque.
    revision: u64,
    /// La voix lisait à l'ouverture de l'éditeur : elle repartira.
    resume: bool,
    font: EditorFont,
    _input: gpui::Subscription,
}

impl EventEmitter<EditorEvent> for TextEditor {}

impl TextEditor {
    pub fn new(session: Arc<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx));
        let subscription = cx.subscribe_in(&input, window, |_, _, event, _, cx| {
            // ⌘↩ (Ctrl+↩ ailleurs) valide, comme dans l'app Mac ; Entrée seule
            // va à la ligne.
            if let InputEvent::PressEnter {
                secondary: true, ..
            } = event
            {
                cx.emit(EditorEvent::Done);
            }
        });
        TextEditor {
            session,
            input,
            origin: String::new(),
            source: String::new(),
            revision: 0,
            resume: false,
            font: EditorFont {
                family: "Literata".into(),
                size: 20.0,
                line_height: 32.0,
            },
            _input: subscription,
        }
    }

    /// Prépare l'éditeur sur `draft` et lui donne le focus, curseur au début.
    pub fn begin(
        &mut self,
        draft: Draft,
        revision: u64,
        resume: bool,
        font: EditorFont,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.origin = draft.markdown;
        self.source = draft.source;
        self.revision = revision;
        self.resume = resume;
        self.font = font;
        let text = self.origin.clone();
        self.input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.set_selected_range(0..0, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// La révision du document en cours de modification.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Le texte tel qu'il est écrit.
    pub fn text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Remplace le texte écrit (les essais, comme `draft` dans ceux de l'app
    /// Mac).
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
    }

    /// La source sous laquelle le texte se rouvrira.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Valide : le texte écrit remplace le document, reprise à `previous`.
    /// `resume` faux : la voix reste en pause.
    pub fn finish(&mut self, previous: usize, resume: bool, cx: &mut Context<Self>) -> Outcome {
        let draft = self.text(cx);
        let outcome = apply(
            &self.session,
            &self.origin,
            &draft,
            &self.source,
            previous,
            resume && self.resume,
        );
        self.resume = false;
        outcome
    }

    /// Annule : rien ne change, la voix repart si elle lisait.
    pub fn cancel(&mut self) {
        if self.resume {
            self.session.send(Request::Command(Command::TogglePlay));
        }
        self.resume = false;
    }
}

impl Render for TextEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        use dysflow::tr;
        let font = self.font.clone();
        let keycap = |label: &'static str| {
            div()
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(5.0))
                .bg(p.keycap())
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.chrome())
                .child(label)
        };
        div()
            .id("editor")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .gap(px(12.0))
            .px(px(32.0))
            .pt(px(24.0))
            .pb(px(12.0))
            .bg(p.background())
            // Échap : le champ le laisse passer, l'éditeur le prend et valide
            // (« la saisie se valide », comme l'app Mac).
            .on_action(cx.listener(|_, _: &Escape, _, cx| cx.emit(EditorEvent::Done)))
            // ⌘↩ : le champ émet `PressEnter` puis laisse passer la touche ;
            // non arrêtée, la plateforme la taperait ensuite comme du texte.
            // Entrée seule ne remonte pas : le champ y va à la ligne.
            .on_action(cx.listener(|_, _: &Enter, _, _| {}))
            .child(
                div()
                    .debug_selector(|| "editor-text".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .child(
                        Textarea::new(&self.input)
                            .h_full()
                            .w_full()
                            .bordered(false)
                            .p(px(16.0))
                            .rounded(px(10.0))
                            // En édition, le fond se teinte : on voit qu'on écrit.
                            .bg(p.editing())
                            .font_family(font.family)
                            .text_size(px(font.size))
                            .line_height(px(font.line_height))
                            .text_color(p.text())
                            .aria_label(tr!("Text to edit", "Texte à modifier")),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "editor-bar".to_string())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.0))
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .h(px(40.0))
                    .text_size(px(13.0))
                    .text_color(p.chrome())
                    .child(
                        Icon::new(IconName::Pencil)
                            .with_size(px(14.0))
                            .text_color(p.chrome()),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("Editing the text", "Modification du texte")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(keycap(tr!("Esc", "Échap")))
                            .child(tr!("confirms", "valide")),
                    )
                    .child(
                        div().debug_selector(|| "editor-cancel".to_string()).child(
                            Button::new("editor-cancel")
                                .ghost()
                                .label(tr!("Cancel", "Annuler"))
                                .with_size(gpui_component::Size::Medium)
                                .h(px(32.0))
                                .rounded(px(10.0))
                                .tab_stop(false)
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancel))),
                        ),
                    )
                    .child(
                        div().debug_selector(|| "editor-done".to_string()).child(
                            Button::new("editor-done")
                                .primary()
                                .label(tr!("Done", "Terminé"))
                                .with_size(gpui_component::Size::Medium)
                                .h(px(32.0))
                                .rounded(px(10.0))
                                .tab_stop(false)
                                .tooltip(format!(
                                    "{} ({} {} {})",
                                    tr!("Apply the changes", "Appliquer les modifications"),
                                    tr!("Esc", "Échap"),
                                    tr!("or", "ou"),
                                    crate::inputs::command_label("↩")
                                ))
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Done))),
                        ),
                    ),
            )
    }
}

impl Focusable for TextEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

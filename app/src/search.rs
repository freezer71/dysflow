//! La barre de recherche : un champ de gpui-component, le rang du résultat
//! (« 1 sur 3 », « Aucun résultat »), précédent, suivant et fermer.
//!
//! Comme l'app Mac, la recherche part à Entrée, pas à chaque lettre : la
//! lecture ne saute pas d'un mot à l'autre pendant qu'on tape. Entrée sur la
//! requête déjà cherchée passe au résultat suivant, Maj+Entrée au précédent ;
//! puis le focus revient à la lecture, où N et P avancent. Échap dans le
//! champ annule la recherche et ferme la barre (la fenêtre s'en charge, sur
//! `SearchEvent::Cancel`).
//!
//! Tout part au lecteur par `Session::send` ; le rang vient de l'instantané
//! publié, jamais d'une question posée au lecteur.

use std::sync::Arc;

use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::view::{SearchState, Snapshot};
use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_component::{Disableable, Icon, Sizable};
use gpui_kit_assets::IconName;

use crate::colors;
use crate::session::Session;

/// Ce que la barre demande à la fenêtre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    /// La requête est partie : le focus retourne à la lecture.
    Submitted,
    /// Un chevron : la lecture suit le résultat, le focus ne bouge pas.
    Moved,
    /// Échap dans le champ : la recherche s'annule, la barre se ferme.
    Cancel,
}

/// Une requête envoyée au lecteur, en attente de sa réponse.
#[derive(Debug, Clone, PartialEq)]
struct Sent {
    query: String,
    /// Le message du lecteur au moment de l'envoi : sans résultat, le
    /// lecteur ne publie aucune recherche mais change son message.
    status: String,
    /// Vrai une fois le lecteur passé par la requête.
    answered: bool,
}

/// Le rang à afficher : « 1 sur 3 » quand la recherche publiée est celle
/// qu'on a envoyée (ou qu'aucune n'est en attente), « Aucun résultat » quand
/// le lecteur a répondu sans en publier, rien sinon.
pub fn result_label(
    search: Option<&SearchState>,
    sent: Option<&str>,
    answered: bool,
) -> Option<String> {
    match (search, sent) {
        (Some(s), None) => Some(of(s)),
        (Some(s), Some(q)) if s.query == q => Some(of(s)),
        (None, Some(_)) if answered => Some(dysflow::tr!("No match", "Aucun résultat").to_string()),
        _ => None,
    }
}

fn of(s: &SearchState) -> String {
    dysflow::tr!(en = "{} of {}", fr = "{} sur {}", s.position, s.count)
}

pub struct SearchBar {
    session: Arc<Session>,
    input: Entity<InputState>,
    /// La recherche publiée par le lecteur.
    search: Option<SearchState>,
    status: String,
    /// La révision du document publié : un nouveau document perd la
    /// recherche, et ce qu'on savait de la requête envoyée.
    revision: u64,
    sent: Option<Sent>,
    /// Le rang affiché, gardé pour ne redessiner que s'il change.
    result: Option<String>,
    _input: gpui::Subscription,
}

impl EventEmitter<SearchEvent> for SearchBar {}

impl SearchBar {
    pub fn new(session: Arc<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(dysflow::tr!("Search", "Rechercher"))
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event, _, cx| {
            if let InputEvent::PressEnter { shift, .. } = event {
                this.submit(*shift, cx);
            }
        });
        let snapshot = session.snapshot();
        SearchBar {
            session,
            input,
            search: snapshot.search,
            status: snapshot.status,
            revision: snapshot.revision,
            sent: None,
            result: None,
            _input: subscription,
        }
    }

    /// La requête telle qu'elle est tapée.
    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Le rang affiché : « 1 sur 3 », « Aucun résultat », ou rien.
    pub fn result(&self) -> Option<String> {
        self.result.clone()
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    /// Donne le focus au champ, sa requête sélectionnée : une frappe la
    /// remplace, Entrée la relance.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// Entrée : la requête part ; déjà cherchée (le lecteur a une
    /// recherche en cours pour elle), on passe au résultat suivant (au
    /// précédent avec Maj). Sinon elle repart toujours, comme dans l'app
    /// Mac : le lecteur a pu perdre sa recherche (nouveau document, repos).
    fn submit(&mut self, back: bool, cx: &mut Context<Self>) {
        let query = self.query(cx).trim().to_string();
        if query.is_empty() {
            return;
        }
        let delta = if back { -1 } else { 1 };
        if self.search.as_ref().is_some_and(|s| s.query == query) {
            self.session
                .send(Request::Command(Command::SearchAdvance(delta)));
        } else {
            self.session
                .send(Request::Command(Command::Search(query.clone())));
            // La même requête, déjà répondue sans résultat sur ce document
            // (`sent` est oublié quand la révision change) : le lecteur
            // répondra le même message, qu'on ne verrait pas changer. La
            // réponse connue reste valable.
            let answered = self
                .sent
                .as_ref()
                .is_some_and(|s| s.query == query && s.answered);
            self.sent = Some(Sent {
                query,
                status: self.status.clone(),
                answered,
            });
            self.refresh(cx);
        }
        cx.emit(SearchEvent::Submitted);
    }

    /// Échap ou le bouton « fermer » : la recherche s'annule ; la barre
    /// rouverte repartira vide.
    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.send(Request::Command(Command::SearchCancel));
        self.sent = None;
        self.search = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh(cx);
    }

    fn advance(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.session
            .send(Request::Command(Command::SearchAdvance(delta)));
        cx.emit(SearchEvent::Moved);
    }

    /// Le nouvel instantané publié ; ne redessine que si le rang change.
    pub fn receive(&mut self, snapshot: &Snapshot, cx: &mut Context<Self>) {
        // Le lecteur a perdu la recherche sans qu'on l'annule (nouveau
        // document, mise au repos) : la réponse à la requête envoyée ne vaut
        // plus, sauf si c'est une autre requête qui l'a remplacée.
        let lost = match (&self.search, &snapshot.search, &self.sent) {
            (Some(old), None, Some(sent)) => old.query == sent.query,
            _ => false,
        };
        if snapshot.revision != self.revision || lost {
            self.sent = None;
        }
        self.revision = snapshot.revision;
        if let Some(sent) = self.sent.as_mut() {
            let found = snapshot
                .search
                .as_ref()
                .is_some_and(|s| s.query == sent.query);
            if found || snapshot.status != sent.status {
                sent.answered = true;
            }
        }
        self.search = snapshot.search.clone();
        self.status = snapshot.status.clone();
        // Ne prévient que si le rang affiché change.
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let result = result_label(
            self.search.as_ref(),
            self.sent.as_ref().map(|s| s.query.as_str()),
            self.sent.as_ref().is_some_and(|s| s.answered),
        );
        if result != self.result {
            self.result = result;
            cx.notify();
        }
    }

    fn nav_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: String,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div().debug_selector(|| id.to_string()).child(
            Button::new(id)
                .ghost()
                .icon(icon)
                .with_size(px(16.0))
                .size(px(32.0))
                .rounded(px(10.0))
                .tab_stop(false)
                .tooltip(tooltip)
                .disabled(self.search.is_none())
                .on_click(cx.listener(move |this, _, _, cx| this.advance(delta, cx))),
        )
    }
}

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let missing = self.search.is_none() && self.result.is_some();
        let previous = self.nav_button(
            "search-previous",
            IconName::ChevronUp,
            format!(
                "{} (⇧P, ⇧↩)",
                dysflow::tr!("Previous match", "Résultat précédent")
            ),
            -1,
            cx,
        );
        let next = self.nav_button(
            "search-next",
            IconName::ChevronDown,
            format!("{} (⇧N, ↩)", dysflow::tr!("Next match", "Résultat suivant")),
            1,
            cx,
        );
        div()
            .id("search-bar")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.0))
            .h(px(48.0))
            .px(px(16.0))
            .bg(colors::surface())
            .border_b_1()
            .border_color(colors::border())
            .text_size(px(13.0))
            .text_color(colors::chrome())
            // Échap dans le champ : le champ le laisse passer, la barre le
            // prend.
            .on_action(cx.listener(|_, _: &Escape, _, cx| cx.emit(SearchEvent::Cancel)))
            // Entrée : le champ émet `PressEnter` puis laisse passer la
            // touche. Non arrêtée, la plateforme la tape ensuite comme du
            // texte (« \n », filtré en une ligne) : une requête sélectionnée
            // était remplacée par rien. La barre l'arrête ici.
            .on_action(cx.listener(|_, _: &Enter, _, _| {}))
            .child(
                div().w(px(320.0)).flex_none().child(
                    Input::new(&self.input)
                        .with_size(gpui_component::Size::Medium)
                        .prefix(
                            Icon::new(IconName::Search)
                                .with_size(px(16.0))
                                .text_color(colors::muted()),
                        )
                        .aria_label(dysflow::tr!("Search the text", "Rechercher dans le texte")),
                ),
            )
            .child(
                div()
                    .debug_selector(|| "search-result".to_string())
                    .min_w(px(112.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if missing {
                        colors::error()
                    } else {
                        colors::chrome()
                    })
                    .when(missing, |this| {
                        this.child(
                            Icon::new(IconName::CircleAlert)
                                .with_size(px(14.0))
                                .text_color(colors::error()),
                        )
                    })
                    .children(self.result.clone()),
            )
            .child(previous)
            .child(next)
            .child(div().flex_1())
            .child(
                div().debug_selector(|| "search-close".to_string()).child(
                    Button::new("search-close")
                        .ghost()
                        .icon(IconName::Close)
                        .with_size(px(16.0))
                        .size(px(32.0))
                        .rounded(px(10.0))
                        .tab_stop(false)
                        .tooltip(format!(
                            "{} ({})",
                            dysflow::tr!("Close the search", "Fermer la recherche"),
                            dysflow::tr!("Esc", "Échap")
                        ))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SearchEvent::Cancel))),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(query: &str, position: usize, count: usize) -> SearchState {
        SearchState {
            query: query.into(),
            position,
            count,
        }
    }

    #[test]
    fn the_result_follows_what_was_sent() {
        let deux = state("deux", 1, 3);
        let of = |p, c| dysflow::tr!(en = "{} of {}", fr = "{} sur {}", p, c);
        assert_eq!(result_label(Some(&deux), None, false), Some(of(1, 3)));
        assert_eq!(
            result_label(Some(&deux), Some("deux"), true),
            Some(of(1, 3))
        );
        // Une autre requête en attente : l'ancien rang ne se montre plus.
        assert_eq!(result_label(Some(&deux), Some("trois"), false), None);
        // Le lecteur a répondu sans rien publier : aucun résultat.
        let none = dysflow::tr!("No match", "Aucun résultat").to_string();
        assert_eq!(result_label(None, Some("zzz"), true), Some(none));
        assert_eq!(result_label(None, Some("zzz"), false), None);
        assert_eq!(result_label(None, None, false), None);
    }
}

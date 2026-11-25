//! Le sommaire : la liste des titres du document (`List` de gpui-component),
//! en retrait selon leur niveau, la section lue marquée d'un repère et en
//! gras (pas seulement par la couleur).
//!
//! ↑ ↓ parcourent la liste quand elle a le focus, Entrée ou un clic mènent
//! au titre (`Command::SeekTo`) ; un titre sans phrase est désactivé : ni
//! clic ni Entrée n'envoient rien. Hors du clavier, la ligne choisie suit
//! la section lue, comme dans l'app Mac.

use std::sync::Arc;

use dysflow::reader::command::Command;
use dysflow::reader::handle::Request;
use dysflow::reader::view::{DocView, SectionView};
use gpui::{
    div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, Window,
};
use gpui_component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_component::IndexPath;

use crate::colors;
use crate::session::Session;

/// Le retrait par niveau de titre.
const INDENT: f32 = 14.0;
/// La hauteur d'une ligne : toutes les lignes d'une `List` ont la même.
const ROW: f32 = 32.0;

/// Une ligne du sommaire.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentsRow {
    pub section: usize,
    pub title: String,
    pub level: u8,
    /// La première phrase du titre ; `None` : rien à lire, ligne désactivée.
    pub target: Option<usize>,
    /// La section lue en ce moment.
    pub current: bool,
}

impl ContentsRow {
    pub fn enabled(&self) -> bool {
        self.target.is_some()
    }
}

/// Les lignes du sommaire. La section implicite du début (niveau 0) n'est
/// pas un titre : elle n'y est que si du texte précède le premier titre.
pub fn rows(sections: &[SectionView], current: Option<usize>) -> Vec<ContentsRow> {
    sections
        .iter()
        .filter(|s| s.level > 0 || s.first_sentence.is_some())
        .map(|s| ContentsRow {
            section: s.id,
            title: s.title.clone(),
            level: s.level,
            target: s.first_sentence,
            current: Some(s.id) == current,
        })
        .collect()
}

/// Ce que le sommaire demande à la fenêtre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentsEvent {
    /// Un titre choisi : la lecture y part, le focus revient à la lecture.
    Chosen(usize),
    /// Échap dans la liste : le sommaire se ferme.
    Close,
}

pub struct ContentsDelegate {
    rows: Vec<ContentsRow>,
    selected: Option<usize>,
}

impl ListDelegate for ContentsDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let i = ix.row;
        let row = self.rows.get(i)?;
        let enabled = row.enabled();
        let selected = self.selected == Some(i);
        let indent = f32::from(row.level.saturating_sub(1)) * INDENT;
        // Le repère de la section lue : une barre d'accent à gauche, avec
        // le titre en gras.
        let marker = div()
            .flex_none()
            .w(px(3.0))
            .h(px(16.0))
            .rounded(px(2.0))
            .when(row.current, |this| {
                this.bg(colors::accent())
                    .debug_selector(|| "contents-marker".to_string())
            });
        let title = div()
            .debug_selector(move || format!("contents-title-{i}"))
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .ml(px(indent))
            .font_weight(if row.current {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if enabled {
                colors::text()
            } else {
                colors::disabled()
            })
            .child(if row.title.trim().is_empty() {
                dysflow::tr!("Untitled", "Sans titre").to_string()
            } else {
                row.title.clone()
            });
        let label = if row.current {
            dysflow::tr!(
                en = "{} (being read)",
                fr = "{} (en cours de lecture)",
                row.title
            )
        } else {
            row.title.clone()
        };
        Some(
            ListItem::new(("contents-row", i))
                .debug_selector(move || format!("contents-row-{i}"))
                .accessibility_label(label)
                .disabled(!enabled)
                .h(px(ROW))
                .px(px(8.0))
                .py(px(0.0))
                .rounded(px(8.0))
                .text_size(px(14.0))
                // Une ligne désactivée n'a pas de fond choisi : un filet
                // dit où est la sélection au clavier.
                .when(!enabled && selected, |this| {
                    this.border_1().border_color(colors::border_strong())
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .when(!enabled, |this| {
                            this.debug_selector(move || format!("contents-disabled-{i}"))
                        })
                        .child(marker)
                        .child(title),
                ),
        )
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .px(px(16.0))
            .py(px(8.0))
            .text_size(px(13.0))
            .text_color(colors::muted())
            .child(dysflow::tr!("No headings", "Aucun titre"))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix.map(|ix| ix.row);
    }
}

pub struct ContentsPanel {
    session: Arc<Session>,
    list: Entity<ListState<ContentsDelegate>>,
    document: Option<Arc<DocView>>,
    current: Option<usize>,
    _list: gpui::Subscription,
}

impl EventEmitter<ContentsEvent> for ContentsPanel {}

impl ContentsPanel {
    pub fn new(session: Arc<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = ContentsDelegate {
            rows: Vec::new(),
            selected: None,
        };
        let list = cx.new(|cx| ListState::new(delegate, window, cx));
        let subscription = cx.subscribe(&list, |this: &mut Self, list, event: &ListEvent, cx| {
            match event {
                ListEvent::Confirm(ix) => {
                    let target = list
                        .read(cx)
                        .delegate()
                        .rows
                        .get(ix.row)
                        .and_then(|r| r.target);
                    // Un titre sans phrase ne mène nulle part.
                    if let Some(sentence) = target {
                        this.session
                            .send(Request::Command(Command::SeekTo(sentence)));
                        cx.emit(ContentsEvent::Chosen(sentence));
                    }
                }
                ListEvent::Cancel => cx.emit(ContentsEvent::Close),
                ListEvent::Select(_) => {}
            }
        });
        ContentsPanel {
            session,
            list,
            document: None,
            current: None,
            _list: subscription,
        }
    }

    pub fn rows<'a>(&self, cx: &'a App) -> &'a [ContentsRow] {
        &self.list.read(cx).delegate().rows
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.list.focus_handle(cx)
    }

    /// Choisit la ligne `row` (sans y aller).
    pub fn select(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            list.set_selected_index(Some(IndexPath::new(row)), window, cx);
            list.scroll_to_selected_item(window, cx);
        });
    }

    /// Donne le focus à la liste, la section lue choisie.
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.current_row(cx);
        self.list.update(cx, |list, cx| {
            list.set_selected_index(current.map(IndexPath::new), window, cx);
            list.scroll_to_selected_item(window, cx);
            list.focus(window, cx);
        });
    }

    fn current_row(&self, cx: &App) -> Option<usize> {
        self.rows(cx).iter().position(|r| r.current)
    }

    /// Le document et la section lue ; ne redessine que s'ils changent.
    pub fn receive(
        &mut self,
        document: &Arc<DocView>,
        section: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_document = self
            .document
            .as_ref()
            .is_some_and(|d| Arc::ptr_eq(d, document));
        if same_document && self.current == section {
            return;
        }
        self.document = Some(Arc::clone(document));
        self.current = section;
        let rows = rows(&document.sections, section);
        let browsing = self.focus_handle(cx).is_focused(window);
        self.list.update(cx, |list, cx| {
            if list.delegate().rows != rows {
                list.delegate_mut().rows = rows;
                // Hors du clavier, la ligne choisie suit la section lue.
                if !browsing {
                    let current = list.delegate().rows.iter().position(|r| r.current);
                    list.set_selected_index(current.map(IndexPath::new), window, cx);
                }
                cx.notify();
            }
        });
    }
}

impl Render for ContentsPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("contents")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(256.0))
            .h_full()
            .bg(colors::surface())
            .border_r_1()
            .border_color(colors::border())
            .child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(6.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors::chrome())
                    .child(dysflow::tr!("Contents", "Sommaire")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .px(px(8.0))
                    .pb(px(8.0))
                    .child(List::new(&self.list)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: usize, level: u8, title: &str, first: Option<usize>) -> SectionView {
        SectionView {
            id,
            level,
            title: title.into(),
            first_sentence: first,
        }
    }

    #[test]
    fn rows_skip_the_empty_start_and_mark_the_current_section() {
        let sections = [
            section(0, 0, "Introduction", None),
            section(1, 1, "Un", Some(0)),
            section(2, 2, "Image", None),
            section(3, 1, "Deux", Some(4)),
        ];
        let shown = rows(&sections, Some(3));
        let titles: Vec<&str> = shown.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Un", "Image", "Deux"]);
        assert_eq!(
            shown.iter().map(|r| r.current).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert!(!shown[1].enabled());
        assert!(shown[0].enabled() && shown[2].enabled());
        // Du texte avant le premier titre : la section implicite se lit.
        let with_intro = [
            section(0, 0, "Introduction", Some(0)),
            section(1, 1, "Un", Some(2)),
        ];
        assert_eq!(rows(&with_intro, Some(0)).len(), 2);
    }
}

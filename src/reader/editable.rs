//! Le texte qu'on modifie en mode édition : celui qu'on a ouvert, tel quel.
//! Sans lui, le document est réécrit en Markdown depuis ses blocs : titres,
//! listes, citations, code et tableaux gardent leur forme.
//!
//! Port de `macos/Sources/Model/EditableText.swift`.

use super::view::{BlockKindView, DocView};
use super::Reader;

/// Ce que le mode édition montre : le texte ouvert, sinon le document
/// réécrit en Markdown. `None` sans document.
pub fn editable_markdown(reader: &Reader) -> Option<String> {
    if let Some(text) = reader.text_markdown() {
        return Some(text.to_string());
    }
    let kept = reader.session().is_some() || reader.resting_document().is_some();
    kept.then(|| markdown_from(&super::view::document(reader)))
}

/// La source sous laquelle le document gardé a été ouvert, telle qu'elle a
/// été donnée (« clipboard », le chemin d'un fichier) : le texte modifié se
/// rouvre sous la même. `None` sans document.
pub fn editable_source(reader: &Reader) -> Option<String> {
    reader.text.as_ref().map(|t| t.origin.clone())
}

/// Le document réécrit en Markdown, bloc par bloc.
pub fn markdown_from(doc: &DocView) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut table: Vec<String> = Vec::new();

    fn flush_table(parts: &mut Vec<String>, table: &mut Vec<String>) {
        if table.is_empty() {
            return;
        }
        parts.push(table.join("\n"));
        table.clear();
    }

    for block in &doc.blocks {
        let lines = &block.display_lines;
        let joined = lines.join(" ");
        if let BlockKindView::TableRow { header } = block.kind {
            table.push(format!("| {} |", lines.join(" | ")));
            if header {
                table.push(format!("|{}", " --- |".repeat(lines.len().max(1))));
            }
            continue;
        }
        flush_table(&mut parts, &mut table);
        match &block.kind {
            BlockKindView::Heading { level } => {
                parts.push(format!("{} {joined}", "#".repeat((*level).max(1) as usize)));
            }
            BlockKindView::ListItem { depth, ordinal } => {
                let indent = "  ".repeat(*depth as usize);
                let marker = ordinal.map_or_else(|| "-".to_string(), |n| format!("{n}."));
                parts.push(format!("{indent}{marker} {joined}"));
            }
            BlockKindView::Quote => {
                let quoted: Vec<String> = lines.iter().map(|l| format!("> {l}")).collect();
                parts.push(quoted.join("\n"));
            }
            BlockKindView::Code { language } => {
                parts.push(format!(
                    "```{}\n{}\n```",
                    language.as_deref().unwrap_or(""),
                    lines.join("\n")
                ));
            }
            BlockKindView::Paragraph | BlockKindView::TableRow { .. } => parts.push(joined),
        }
    }
    flush_table(&mut parts, &mut table);
    parts.join("\n\n")
}

/// La phrase où reprendre après une modification : la même, ou la dernière
/// si le texte a raccourci ; `None` s'il n'a plus de phrase.
pub fn resume_sentence(previous: usize, new_count: usize) -> Option<usize> {
    new_count.checked_sub(1).map(|last| previous.min(last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::build::silent_reader;
    use crate::reader::view::{self, BlockView};
    use std::time::Instant;

    fn block(kind: BlockKindView, lines: &[&str]) -> BlockView {
        BlockView {
            kind,
            display_lines: lines.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn headings_lists_quotes_code_tables_keep_their_form() {
        let doc = DocView {
            blocks: vec![
                block(BlockKindView::Heading { level: 1 }, &["Titre"]),
                block(
                    BlockKindView::Paragraph,
                    &["Une phrase", "sur deux lignes."],
                ),
                block(
                    BlockKindView::ListItem {
                        depth: 1,
                        ordinal: None,
                    },
                    &["item"],
                ),
                block(
                    BlockKindView::ListItem {
                        depth: 0,
                        ordinal: Some(1),
                    },
                    &["item"],
                ),
                block(BlockKindView::Quote, &["cite", "encore"]),
                block(
                    BlockKindView::Code {
                        language: Some("rust".into()),
                    },
                    &["fn a() {", "    b();", "}"],
                ),
                block(BlockKindView::TableRow { header: true }, &["Nom", "Âge"]),
                block(BlockKindView::TableRow { header: false }, &["Léa", "9"]),
                block(BlockKindView::Paragraph, &["Fin."]),
            ],
            ..Default::default()
        };
        assert_eq!(
            markdown_from(&doc),
            "# Titre\n\n\
             Une phrase sur deux lignes.\n\n  \
             - item\n\n\
             1. item\n\n\
             > cite\n> encore\n\n\
             ```rust\nfn a() {\n    b();\n}\n```\n\n\
             | Nom | Âge |\n| --- | --- |\n| Léa | 9 |\n\n\
             Fin."
        );
    }

    #[test]
    fn a_heading_without_level_and_code_without_language_stay_markdown() {
        let doc = DocView {
            blocks: vec![
                block(BlockKindView::Heading { level: 0 }, &["Titre"]),
                block(BlockKindView::Code { language: None }, &["x"]),
                // Une ligne de tableau d'en-tête sans cellule garde un filet.
                block(BlockKindView::TableRow { header: true }, &[]),
            ],
            ..Default::default()
        };
        assert_eq!(
            markdown_from(&doc),
            "# Titre\n\n```\nx\n```\n\n|  |\n| --- |"
        );
    }

    #[test]
    fn an_empty_document_is_an_empty_text() {
        assert_eq!(markdown_from(&DocView::default()), "");
    }

    #[test]
    fn resume_sentence_clamps() {
        assert_eq!(resume_sentence(5, 3), Some(2));
        assert_eq!(resume_sentence(5, 0), None);
        assert_eq!(resume_sentence(1, 3), Some(1));
        assert_eq!(resume_sentence(0, 1), Some(0));
    }

    /// `testEditingStartsFromTheOpenedText` : le texte ouvert, tel quel.
    #[test]
    fn editing_starts_from_the_opened_text() {
        let mut r = silent_reader();
        assert_eq!(editable_markdown(&r), None);
        assert_eq!(editable_source(&r), None);
        r.open_text("# Titre\n\nUn. Deux.", "clipboard", Instant::now());
        assert_eq!(
            editable_markdown(&r).as_deref(),
            Some("# Titre\n\nUn. Deux.")
        );
        assert_eq!(editable_source(&r).as_deref(), Some("clipboard"));
    }

    /// Le texte modifié rouvert sous sa source garde la même : elle ne
    /// s'allonge pas d'une empreinte à chaque modification.
    #[test]
    fn the_source_stays_the_same_from_one_edit_to_the_next() {
        let mut r = silent_reader();
        r.open_text("Un.", "clipboard", Instant::now());
        let source = editable_source(&r).unwrap();
        r.open_text("Un. Deux.", &source, Instant::now());
        assert_eq!(editable_source(&r).as_deref(), Some("clipboard"));
        assert_eq!(editable_markdown(&r).as_deref(), Some("Un. Deux."));
    }

    /// Un texte au repos reste modifiable ; fermé, il n'y a plus rien.
    #[test]
    fn a_resting_text_stays_editable_and_a_cleared_one_does_not() {
        let mut r = silent_reader();
        r.open_text("Un. Deux.", "clipboard", Instant::now());
        r.rest();
        assert_eq!(editable_markdown(&r).as_deref(), Some("Un. Deux."));
        r.clear(Instant::now());
        assert_eq!(editable_markdown(&r), None);
        assert_eq!(editable_source(&r), None);
    }

    /// `testTheDocumentIsRewrittenWhenThereIsNoSource` : réécrit depuis ses
    /// blocs, titres et listes compris.
    #[test]
    fn the_document_is_rewritten_when_there_is_no_source() {
        let mut r = silent_reader();
        r.open_text(
            "# Titre\n\n- un\n- deux\n\nTexte final.",
            "test",
            Instant::now(),
        );
        let markdown = markdown_from(&view::document(&r));
        assert!(markdown.starts_with("# Titre"), "{markdown}");
        assert!(markdown.contains("- un"), "{markdown}");
        assert!(markdown.contains("Texte final."), "{markdown}");
    }
}

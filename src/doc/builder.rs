//! Construction incrémentale du document à partir de markdown.
//!
//! Le constructeur est nourri bloc par bloc, ce qui permet de commencer à lire
//! un flux avant d'en avoir vu la fin. L'état de section survit d'un bloc à
//! l'autre, si bien que le résultat est identique que le document arrive d'un
//! coup ou par morceaux.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::{Block, BlockKind, Document, Emphasis, Punctuation, Section, Sentence, Word};
use crate::text::numerals::{section_number, verbalize_section_number};
use crate::text::{sanitize, sanitize_layout, split_sentences, verbalize};

/// Traitement réservé aux blocs de code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodePolicy {
    /// Annonce « bloc de code <langage> » sans en lire le contenu.
    Announce,
    /// Ignore le bloc entièrement.
    Skip,
    /// Lit le code tel quel.
    Read,
}

impl CodePolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "announce" | "annonce" => Some(CodePolicy::Announce),
            "skip" | "ignore" => Some(CodePolicy::Skip),
            "read" | "lire" => Some(CodePolicy::Read),
            _ => None,
        }
    }
}

/// Largeur de chaque colonne, en caractères.
fn column_widths(rows: &[(bool, Vec<String>)]) -> Vec<usize> {
    let mut widths: Vec<usize> = Vec::new();
    for (_, cells) in rows {
        for (i, cell) in cells.iter().enumerate() {
            let len = cell.chars().count();
            match widths.get_mut(i) {
                Some(width) => *width = (*width).max(len),
                None => widths.push(len),
            }
        }
    }
    widths
}

/// Aligne une ligne sur les largeurs données, sans remplir la dernière colonne.
///
/// Un filet sépare les colonnes : l'alignement seul laisse l'œil glisser d'une
/// ligne à l'autre au milieu d'un tableau.
fn align_row(cells: &[String], widths: &[usize]) -> String {
    let last = cells.len().saturating_sub(1);
    let mut line = String::new();
    for (i, cell) in cells.iter().enumerate() {
        line.push_str(cell);
        if i < last {
            let pad = widths.get(i).copied().unwrap_or(0).saturating_sub(cell.chars().count());
            line.push_str(&" ".repeat(pad));
            line.push_str(" │ ");
        }
    }
    line
}

/// Filet posé sous l'en-tête, aligné sur les séparateurs des colonnes.
fn header_rule(widths: &[usize]) -> String {
    let last = widths.len().saturating_sub(1);
    let mut rule = String::new();
    for (i, width) in widths.iter().enumerate() {
        // Le filet couvre la colonne et l'espace qui l'écarte du séparateur.
        let span = match (i == 0, i == last) {
            (true, true) => *width,
            (true, false) | (false, true) => width + 1,
            _ => width + 2,
        };
        rule.push_str(&"─".repeat(span));
        if i < last {
            rule.push('┼');
        }
    }
    rule
}

/// Rend à un titre le rang que le nettoyage lui a retiré.
///
/// Un titre se dit avec son rang en toutes lettres, mais s'écrit avec son
/// chiffre : « un, Contexte » à la voix, « 1. Contexte » à l'œil. Sans quoi le
/// sommaire ne ressemble plus au document. Le rang reste hors du texte
/// découpé : « 1. » y passerait pour une phrase entière.
fn numbered(heading: bool, raw: &str, text: String) -> String {
    if !heading {
        return text;
    }
    match section_number(raw) {
        Some(number) if !text.starts_with(number) => format!("{number} {text}"),
        _ => text,
    }
}

fn is_heading(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::Heading { .. })
}

/// Découpe un source en lignes affichables, sans les vides de bordure.
fn source_lines(raw: &str) -> Vec<String> {
    let mut lines: Vec<String> = raw
        .trim_end_matches('\n')
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// Un mot du fichier et la mise en valeur qu'il y portait.
struct Marked {
    text: String,
    emphasis: Emphasis,
}

/// Mises en valeur ouvertes, comptées : elles s'imbriquent.
#[derive(Default)]
struct Emphases {
    strong: usize,
    italic: usize,
    struck: usize,
    link: usize,
    code: usize,
}

impl Emphases {
    fn current(&self) -> Emphasis {
        Emphasis {
            strong: self.strong > 0,
            italic: self.italic > 0,
            struck: self.struck > 0,
            link: self.link > 0,
            code: self.code > 0,
        }
    }
}

/// Rend à un mot la mise en valeur qu'il portait dans le fichier.
///
/// Les deux listes viennent du même texte, mais le nettoyage a pu en retirer
/// un mot — une adresse, un marqueur resté — et un titre y ajoute son rang.
/// On avance donc jusqu'au mot reconnu plutôt qu'à l'aveugle : un gras décalé
/// se remarquerait plus qu'un gras absent.
fn emphasis_of(word: &str, marks: &[Marked], cursor: &mut usize) -> Emphasis {
    /// Au-delà, ce n'est plus un décalage mais un autre texte.
    const LOOKAHEAD: usize = 8;

    let end = (*cursor + LOOKAHEAD).min(marks.len());
    for (i, mark) in marks.iter().enumerate().take(end).skip(*cursor) {
        if mark.text == word {
            *cursor = i + 1;
            return mark.emphasis;
        }
    }
    Emphasis::default()
}

/// Bloc en cours d'assemblage.
struct Pending {
    kind: BlockKind,
    text: String,
    /// Les mots du bloc, dans l'ordre du fichier, avec leur mise en valeur.
    marks: Vec<Marked>,
}

impl Pending {
    /// Ajoute un fragment tel quel, sans espace implicite.
    ///
    /// Les fragments d'un même bloc sont adjacents dans la source : les séparer
    /// coupe les mots. La ponctuation intelligente rend l'apostrophe sous forme
    /// d'événement distinct, si bien que « s'y greffe » devenait « s ’ y
    /// greffe » — trois morceaux, et la liaison perdue à la synthèse.
    ///
    /// Les espaces qui manquent vraiment sont poussés explicitement : les sauts
    /// de ligne souples et durs, et la fin d'un paragraphe interne à un item.
    fn push_text(&mut self, value: &str, emphasis: Emphasis) {
        // Un fragment qui ne commence pas par une espace continue le mot
        // précédent, et en garde la mise en valeur : « **gras**, » arrive en
        // deux morceaux, et la virgule appartient encore au mot.
        let joined = !value.starts_with(char::is_whitespace)
            && self.text.chars().last().is_some_and(|c| !c.is_whitespace());
        self.text.push_str(value);
        for (i, word) in value.split_whitespace().enumerate() {
            match self.marks.last_mut() {
                Some(last) if i == 0 && joined => last.text.push_str(word),
                _ => self.marks.push(Marked { text: word.to_string(), emphasis }),
            }
        }
    }
}

pub struct DocumentBuilder {
    doc: Document,
    code_policy: CodePolicy,
    current_section: usize,
    /// Profondeur de liste courante, pour l'indentation des items.
    list_stack: Vec<Option<u64>>,
    quote_depth: usize,
    pending: Option<Pending>,
    /// Cellules de la ligne de tableau en cours.
    table_cells: Vec<String>,
    /// Lignes du tableau en cours, retenues jusqu'à sa fermeture.
    ///
    /// L'alignement des colonnes ne se connaît qu'une fois toutes les lignes
    /// vues : les émettre au fil de l'eau interdirait de les aligner.
    table_rows: Vec<(bool, Vec<String>)>,
    in_table_head: bool,
    in_table: bool,
    /// Mises en valeur ouvertes : elles traversent les fragments de texte.
    emphases: Emphases,
    word_counter: usize,
    /// Langue du document : la mise en mots des nombres, dates et sigles est
    /// française, et ne vaut que pour un document français.
    language: String,
}

impl DocumentBuilder {
    /// Un document français : la normalisation complète.
    pub fn new(code_policy: CodePolicy) -> Self {
        Self::with_language(code_policy, "fr")
    }

    /// Un document dans `language` (`fr`, `en`, ou un code d'un autre modèle).
    pub fn with_language(code_policy: CodePolicy, language: &str) -> Self {
        let mut doc = Document::default();
        // Section racine : accueille le texte qui précède tout titre.
        doc.sections.push(Section {
            id: 0,
            level: 0,
            title: "Introduction".to_string(),
            first_sentence: None,
            block_ids: Vec::new(),
        });
        Self {
            doc,
            code_policy,
            current_section: 0,
            list_stack: Vec::new(),
            quote_depth: 0,
            pending: None,
            table_cells: Vec::new(),
            table_rows: Vec::new(),
            in_table_head: false,
            in_table: false,
            emphases: Emphases::default(),
            word_counter: 0,
            language: language.trim().to_string(),
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    fn french(&self) -> bool {
        crate::text::language::Lang::parse(&self.language) == Some(crate::text::language::Lang::Fr)
    }

    fn options() -> Options {
        // Sans ENABLE_TABLES, un tableau GFM retombe en paragraphe et ses
        // barres verticales sont lues comme du texte.
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_SMART_PUNCTUATION);
        options
    }

    /// Analyse un fragment markdown et l'ajoute au document.
    pub fn push_block(&mut self, markdown: &str) {
        if markdown.trim().is_empty() {
            return;
        }
        for event in Parser::new_ext(markdown, Self::options()) {
            self.handle(event);
        }
        // Un fragment se termine toujours par un bloc clos.
        self.flush();
        // Un tableau tronqué par la fin du flux vaut mieux que rien.
        self.flush_table();
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush();
                self.pending = Some(Pending {
                    kind: BlockKind::Heading { level: level as u8 },
                    text: String::new(),
                    marks: Vec::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => self.flush(),

            Event::Start(Tag::Paragraph) => {
                // Dans un item de liste ou une citation, le paragraphe ne crée
                // pas de bloc : il prolonge celui qui est déjà ouvert.
                if self.pending.is_none() {
                    let kind = if self.quote_depth > 0 {
                        BlockKind::Quote
                    } else {
                        BlockKind::Paragraph
                    };
                    self.pending =
                        Some(Pending { kind, text: String::new(), marks: Vec::new() });
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if matches!(
                    self.pending.as_ref().map(|p| &p.kind),
                    Some(BlockKind::Paragraph) | Some(BlockKind::Quote)
                ) {
                    self.flush();
                } else if let Some(pending) = self.pending.as_mut() {
                    // Sépare deux paragraphes d'un même item.
                    pending.push_text(" ", Emphasis::default());
                }
            }

            Event::Start(Tag::List(start)) => self.list_stack.push(start),
            Event::End(TagEnd::List(_)) => {
                self.list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                self.flush();
                let depth = self.list_stack.len().saturating_sub(1) as u8;
                let ordinal = self.list_stack.last().copied().flatten();
                // Fait avancer le compteur de la liste ordonnée courante.
                if let Some(slot) = self.list_stack.last_mut() {
                    if let Some(n) = slot.as_mut() {
                        *n += 1;
                    }
                }
                self.pending = Some(Pending {
                    kind: BlockKind::ListItem { depth, ordinal },
                    text: String::new(),
                    marks: Vec::new(),
                });
            }
            Event::End(TagEnd::Item) => self.flush(),

            Event::Start(Tag::BlockQuote(_)) => {
                self.flush();
                self.quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush();
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split(',').next().unwrap_or("").trim().to_string();
                        (!lang.is_empty()).then_some(lang)
                    }
                    CodeBlockKind::Indented => None,
                };
                self.pending = Some(Pending {
                    kind: BlockKind::Code { language },
                    text: String::new(),
                    marks: Vec::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => self.flush(),

            Event::Start(Tag::Table(_)) => {
                self.flush();
                self.in_table = true;
            }
            Event::End(TagEnd::Table) => {
                self.in_table = false;
                self.flush_table();
            }
            Event::Start(Tag::TableHead) => {
                self.in_table_head = true;
                self.table_cells.clear();
            }
            Event::End(TagEnd::TableHead) => {
                self.flush_table_row();
                self.in_table_head = false;
            }
            Event::Start(Tag::TableRow) => self.table_cells.clear(),
            Event::End(TagEnd::TableRow) => self.flush_table_row(),
            Event::Start(Tag::TableCell) => self.table_cells.push(String::new()),
            Event::End(TagEnd::TableCell) => {}

            // La mise en valeur ne se prononce pas — la voix en dirait le
            // marqueur — mais elle se montre : c'est la structure de la phrase.
            Event::Start(Tag::Strong) => self.emphases.strong += 1,
            Event::End(TagEnd::Strong) => self.emphases.strong -= 1,
            Event::Start(Tag::Emphasis) => self.emphases.italic += 1,
            Event::End(TagEnd::Emphasis) => self.emphases.italic -= 1,
            Event::Start(Tag::Strikethrough) => self.emphases.struck += 1,
            Event::End(TagEnd::Strikethrough) => self.emphases.struck -= 1,
            // Seul le texte du lien est lu et montré : l'adresse se dirait
            // caractère par caractère.
            Event::Start(Tag::Link { .. }) => self.emphases.link += 1,
            Event::End(TagEnd::Link) => self.emphases.link -= 1,

            Event::Text(text) => self.push_text(&text),
            Event::Code(text) => {
                self.emphases.code += 1;
                self.push_text(&text);
                self.emphases.code -= 1;
            }
            // Le saut de ligne du fichier est une composition, pas une
            // coquille : l'œil le suit. La voix ne le voit pas — `sanitize`
            // le ramène à un espace.
            Event::SoftBreak | Event::HardBreak => {
                let brk = if self.in_table { " " } else { "\n" };
                self.push_text(brk)
            }
            Event::TaskListMarker(done) => {
                self.push_text(if done { "fait :" } else { "à faire :" })
            }
            // Le HTML brut et les règles horizontales sont du formatage pur.
            Event::Rule | Event::Html(_) | Event::InlineHtml(_) => {}
            _ => {}
        }
    }

    fn push_text(&mut self, value: &str) {
        if self.in_table {
            if let Some(cell) = self.table_cells.last_mut() {
                cell.push_str(value);
            }
            return;
        }
        let emphasis = self.emphases.current();
        if let Some(pending) = self.pending.as_mut() {
            pending.push_text(value, emphasis);
        }
    }

    fn flush_table_row(&mut self) {
        // Les cellules vides sont gardées : c'est ce qui tient l'alignement
        // des colonnes, et l'appariement avec l'en-tête.
        let cells: Vec<String> = self
            .table_cells
            .drain(..)
            .map(|c| c.trim().to_string())
            .collect();
        if cells.iter().all(|c| c.is_empty()) {
            return;
        }
        self.table_rows.push((self.in_table_head, cells));
    }

    /// Émet le tableau retenu : une ligne par bloc, alignée et appariée.
    fn flush_table(&mut self) {
        if self.table_rows.is_empty() {
            return;
        }
        let rows = std::mem::take(&mut self.table_rows);
        let headers = rows
            .iter()
            .find(|(header, _)| *header)
            .map(|(_, cells)| cells.clone())
            .unwrap_or_default();
        let widths = column_widths(&rows);

        let french = self.french();
        for (header, cells) in rows {
            // Une ligne de corps dit la colonne de chaque cellule : « alpha,
            // huit mille quatre-vingts » ne disait pas lequel était le port.
            let spoken = if header || headers.is_empty() {
                cells
                    .iter()
                    .filter(|c| !c.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                cells
                    .iter()
                    .enumerate()
                    .filter(|(_, cell)| !cell.is_empty())
                    .map(|(i, cell)| match headers.get(i) {
                        Some(name) if !name.is_empty() && french => format!("{name} : {cell}."),
                        Some(name) if !name.is_empty() => format!("{name}: {cell}."),
                        _ => format!("{cell}."),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let kind = BlockKind::TableRow { header };
            // L'en-tête emporte son filet : il nomme les colonnes de tout ce
            // qui suit, et le trait le dit sans qu'on ait à le prononcer.
            let mut display = vec![align_row(&cells, &widths)];
            if header {
                display.push(header_rule(&widths));
            }
            // Une cellule ne porte pas de mise en valeur : ce qu'un tableau
            // montre est composé à part, colonnes alignées.
            self.commit(kind, &spoken, display, &[]);
        }
    }

    fn flush(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        // Le code s'affiche tel qu'il est écrit : son indentation porte sa
        // structure, et le nettoyage destiné à la voix la détruirait.
        let mut display = Vec::new();
        let text = match &pending.kind {
            BlockKind::Code { language } => {
                display = source_lines(&pending.text);
                match self.code_policy {
                    CodePolicy::Skip => return,
                    CodePolicy::Read => pending.text.clone(),
                    CodePolicy::Announce => {
                        let (named, bare) = if self.french() {
                            ("Bloc de code", "Bloc de code.")
                        } else {
                            ("Code block", "Code block.")
                        };
                        match language {
                            Some(lang) => format!("{named} {lang}."),
                            None => bare.to_string(),
                        }
                    }
                }
            }
            _ => pending.text.clone(),
        };
        self.commit(pending.kind, &text, display, &pending.marks);
    }

    /// Verse un bloc au document : ce qui se dit, et ce qui s'affiche.
    fn commit(
        &mut self,
        kind: BlockKind,
        raw_text: &str,
        display_lines: Vec<String>,
        marks: &[Marked],
    ) {
        // Ce que l'œil suit : le document tel qu'il est composé — ses lignes,
        // son indentation, l'alignement de ses colonnes. Le balisage seul est
        // parti, le terminal le rendant autrement.
        let mut display_lines = display_lines;
        if display_lines.is_empty() {
            let layout = numbered(is_heading(&kind), raw_text, sanitize_layout(raw_text));
            display_lines = source_lines(&layout);
        }
        // Ce que l'œil lit : le texte nettoyé, chiffres et dates tels qu'ils
        // sont écrits. La mise en mots ne concerne que la voix, et se fait
        // phrase par phrase, plus bas.
        let body = sanitize(raw_text);
        if body.is_empty() {
            return;
        }
        // Un titre se dit avec son rang en toutes lettres, mais s'écrit avec
        // son chiffre : « un, Contexte » à la voix, « 1. Contexte » à l'œil.
        // Sans quoi le sommaire ne ressemble plus au document. Le rang reste
        // hors du texte découpé : « 1. » y passerait pour une phrase entière.
        // La numérotation d'une liste reste retirée, sa structure étant déjà
        // portée par le rang de l'item.
        let shown = numbered(is_heading(&kind), raw_text, body.clone());

        // Un titre ouvre une section, à laquelle il appartient lui-même.
        if let BlockKind::Heading { level } = kind {
            let id = self.doc.sections.len();
            self.doc.sections.push(Section {
                id,
                level,
                title: shown.clone(),
                first_sentence: None,
                block_ids: Vec::new(),
            });
            self.current_section = id;
            if self.doc.title.is_empty() {
                self.doc.title = shown.clone();
            }
        }

        let block_id = self.doc.blocks.len();
        let section_id = self.current_section;
        let pause = kind.trailing_pause_ms();

        let texts = if matches!(kind, BlockKind::Code { .. }) && self.code_policy == CodePolicy::Read
        {
            // Le code n'a pas de phrases : une ligne, une unité de synthèse.
            body.lines().map(str::to_string).filter(|l| !l.trim().is_empty()).collect()
        } else {
            split_sentences(&body)
        };

        let start = self.doc.sentences.len();
        let count = texts.len();
        // Les mots du bloc défilent d'une phrase à l'autre : la mise en valeur
        // vient du fichier, qui n'a pas de phrases.
        let mut mark = 0usize;
        let titled = matches!(kind, BlockKind::Heading { .. }) && count == 1;
        for (i, written) in texts.into_iter().enumerate() {
            let id = self.doc.sentences.len();
            // Les mots affichés viennent du texte écrit ; celui envoyé au
            // moteur en est la forme parlée. Un titre y ajoute son rang, à
            // condition de tenir en une phrase : sinon la correspondance
            // entre les deux ne serait plus sûre.
            let mut words = self.tokenize(if titled { &shown } else { &written });
            for word in &mut words {
                word.emphasis = emphasis_of(&word.text, marks, &mut mark);
            }
            // Un document anglais garde ses chiffres : espeak-ng les lit, et
            // la mise en mots française les dirait en français.
            let text = match (titled, self.french()) {
                (true, true) => verbalize(&sanitize(&verbalize_section_number(raw_text))),
                (true, false) => shown.clone(),
                (false, true) => verbalize(&written),
                (false, false) => written.clone(),
            };
            self.doc.sentences.push(Sentence {
                id,
                block_id,
                section_id,
                text,
                words,
                // Seule la dernière phrase porte la pause du bloc.
                trailing_pause_ms: if i + 1 == count { pause } else { 120 },
            });
        }
        let end = self.doc.sentences.len();
        if start == end {
            return;
        }

        if self.doc.sections[section_id].first_sentence.is_none() {
            self.doc.sections[section_id].first_sentence = Some(start);
        }
        self.doc.sections[section_id].block_ids.push(block_id);
        self.doc.blocks.push(Block {
            id: block_id,
            kind,
            section_id,
            text: shown,
            sentences: (start, end),
            display_lines,
        });
    }

    /// Découpe une phrase en mots affichables, ponctuation comprise.
    fn tokenize(&mut self, sentence: &str) -> Vec<Word> {
        let mut words = Vec::new();
        for token in sentence.split_whitespace() {
            // La ponctuation reste dans le texte affiché ; seule sa nature est
            // extraite, pour l'indicateur et pour le rythme du curseur.
            let punctuation = token.chars().rev().find_map(Punctuation::from_char);
            // Un nombre se dit bien plus longtemps qu'il ne s'écrit : garder
            // sa forme parlée laisse le curseur suivre la voix. Le contexte
            // manque à l'échelle du mot, mais un poids n'a pas à être exact.
            let said = if self.french() { verbalize(token) } else { token.to_string() };
            words.push(Word {
                text: token.to_string(),
                index: self.word_counter,
                punctuation,
                spoken: (said != token).then_some(said),
                emphasis: Emphasis::default(),
            });
            self.word_counter += 1;
        }
        words
    }

    /// Marque la fin de l'entrée.
    pub fn finish(&mut self) {
        self.flush();
        self.flush_table();
        self.doc.complete = true;
        if self.doc.title.is_empty() {
            self.doc.title = self
                .doc
                .blocks
                .first()
                .map(|b| b.text.chars().take(60).collect())
                .unwrap_or_else(|| "Document".to_string());
        }
    }

    /// Instantané du document tel qu'il est actuellement construit.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn into_document(mut self) -> Document {
        self.finish();
        self.doc.total_words = self.word_counter;
        self.doc
    }

    /// Nombre de phrases produites jusqu'ici.
    pub fn sentence_count(&self) -> usize {
        self.doc.sentences.len()
    }

    pub fn sync_word_total(&mut self) {
        self.doc.total_words = self.word_counter;
    }
}

/// Raccourci : analyse un document français complet en une fois.
pub fn parse_document(markdown: &str, code_policy: CodePolicy) -> Document {
    parse_document_in(markdown, code_policy, "fr")
}

/// Analyse un document complet dans `language`.
pub fn parse_document_in(markdown: &str, code_policy: CodePolicy, language: &str) -> Document {
    let mut builder = DocumentBuilder::with_language(code_policy, language);
    for block in crate::input::split_blocks(markdown) {
        builder.push_block(&block);
    }
    builder.into_document()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(md: &str) -> Document {
        parse_document(md, CodePolicy::Announce)
    }

    #[test]
    fn figures_are_shown_as_written_and_said_in_words() {
        let d = doc("Le seuil est de 10 000 euros en 2027.\n");
        let sentence = &d.sentences[0];
        let shown: Vec<&str> = sentence.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(
            shown.join(" "),
            "Le seuil est de 10 000 euros en 2027.",
            "l'écran doit montrer les chiffres tels qu'ils sont écrits"
        );
        assert!(
            sentence.text.contains("deux mille vingt-sept"),
            "la voix doit les dire : {:?}",
            sentence.text
        );
        assert!(!sentence.text.contains("2027"), "parlé : {:?}", sentence.text);
    }

    #[test]
    fn a_figure_weighs_what_it_costs_to_say() {
        // « 2027 » s'écrit en quatre signes et se dit en trois mots : sans
        // corriger son poids, le curseur le quitterait au quart du temps et
        // prendrait un mot d'avance sur la voix.
        let d = doc("2027 arrive.\n");
        let sentence = &d.sentences[0];
        assert_eq!(sentence.words[0].text, "2027");
        assert_eq!(
            sentence.word_at(0.5, 1.0),
            Some(0),
            "à mi-phrase, la voix en est encore au nombre"
        );
    }

    #[test]
    fn a_sentence_stays_short_enough_once_it_is_said() {
        // La limite protège la fenêtre du moteur : elle porte donc sur ce
        // qu'il reçoit, pas sur ce que l'écran montre. Une ligne pleine de
        // montants s'allonge beaucoup en passant en mots.
        let long = "Le plafond passe de 100 000 euros à 1 234 567 euros, \
                    puis à 7 654 321 euros, avant 2 345 678 euros et enfin \
                    8 765 432 euros selon le barème 9 876 543 applicable.";
        let d = doc(&format!("{long}\n"));
        for sentence in &d.sentences {
            assert!(
                sentence.text.chars().count() <= 320,
                "phrase de {} signes envoyée au moteur : {:?}",
                sentence.text.chars().count(),
                sentence.text
            );
        }
    }

    #[test]
    fn list_items_are_never_dropped() {
        let d = doc("# Titre\n\nIntro.\n\n- item alpha\n- item beta\n- item gamma\n");
        let texts: Vec<&str> = d.blocks.iter().map(|b| b.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("item alpha")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("item beta")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("item gamma")), "{texts:?}");
        assert_eq!(
            d.blocks.iter().filter(|b| matches!(b.kind, BlockKind::ListItem { .. })).count(),
            3
        );
    }

    #[test]
    fn nested_lists_record_their_depth() {
        let d = doc("- racine\n  - enfant\n");
        let depths: Vec<u8> = d
            .blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::ListItem { depth, .. } => Some(depth),
                _ => None,
            })
            .collect();
        assert_eq!(depths, vec![0, 1]);
    }

    #[test]
    fn code_blocks_are_announced_not_recited() {
        let d = doc("Avant.\n\n```rust\nfn main() { let secret = 42; }\n```\n\nAprès.\n");
        let all = d.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" | ");
        assert!(!all.contains("secret"), "le code ne doit pas être lu : {all}");
        assert!(all.contains("Bloc de code rust"), "{all}");
        assert!(all.contains("Avant") && all.contains("Après"));
    }

    #[test]
    fn code_blocks_can_be_skipped_entirely() {
        let d = parse_document("```\nx = 1\n```\n\nSuite.\n", CodePolicy::Skip);
        let all = d.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
        assert!(!all.contains("Bloc de code"));
        assert!(all.contains("Suite"));
    }

    #[test]
    fn headings_say_their_numbering_but_show_their_figures() {
        let d = doc("### 3.5 Tarif kilométrique de repli\n");
        assert_eq!(d.blocks[0].text, "3.5 Tarif kilométrique de repli");
        assert_eq!(
            d.sentences[0].text,
            "trois point cinq, Tarif kilométrique de repli"
        );
    }

    #[test]
    fn list_items_still_drop_their_numbering() {
        // Le rang d'un item est porté par la structure, pas par la voix.
        let d = doc("1. premier point\n2. second point\n");
        assert!(d.blocks[0].text.starts_with("premier"), "obtenu: {:?}", d.blocks[0].text);
    }

    #[test]
    fn keeps_elisions_glued_to_their_word() {
        // La ponctuation intelligente de pulldown-cmark émet l'apostrophe comme
        // un fragment à part ; l'espacer coupe l'élision, et le moteur perd la
        // liaison — « s ’ y greffe » se lit en trois morceaux.
        let d = doc("La spécification s'y greffe sans réécriture.\n");
        assert!(
            d.blocks[0].text.contains("s\u{2019}y greffe"),
            "obtenu: {:?}",
            d.blocks[0].text
        );
    }

    fn table_rows(doc: &Document) -> Vec<&Block> {
        doc.blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::TableRow { .. }))
            .collect()
    }

    #[test]
    fn a_table_row_names_the_column_of_each_cell() {
        let d = doc("| Serveur | Port |\n|---|---|\n| alpha | 8080 |\n");
        let rows = table_rows(&d);
        // L'en-tête se dit une fois, tel quel…
        assert_eq!(rows[0].text, "Serveur, Port");
        // …puis chaque cellule est annoncée par sa colonne : « alpha, huit
        // mille quatre-vingts » ne disait pas lequel était le port.
        assert_eq!(rows[1].text, "Serveur : alpha. Port : 8080.");
        let spoken = d.sentences[rows[1].sentences.0..rows[1].sentences.1]
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(spoken, "Serveur : alpha. Port : huit mille quatre-vingts.");
    }

    #[test]
    fn each_cell_becomes_its_own_sentence() {
        let d = doc("| Serveur | Port |\n|---|---|\n| alpha | 8080 |\n");
        let row = table_rows(&d)[1];
        // Une cellule par phrase : les flèches parcourent le tableau cellule
        // par cellule au lieu de tout rejouer.
        assert_eq!(row.sentences.1 - row.sentences.0, 2);
    }

    #[test]
    fn a_table_without_a_header_keeps_its_cells_side_by_side() {
        // Sans en-tête, rien à apparier : les cellules restent énumérées.
        let d = doc("| a | b |\n|---|---|\n");
        let rows = table_rows(&d);
        assert_eq!(rows[0].text, "a, b");
    }

    #[test]
    fn table_columns_are_aligned_on_screen() {
        let d = doc("| Route | Méthode |\n|---|---|\n| /users | GET |\n| /a | DELETE |\n");
        let rows = table_rows(&d);
        assert_eq!(
            rows[0].display_lines,
            vec!["Route  │ Méthode", "───────┼────────"],
            "l'en-tête pose les colonnes et le filet qui les tient"
        );
        assert_eq!(rows[1].display_lines, vec!["/users │ GET"]);
        assert_eq!(rows[2].display_lines, vec!["/a     │ DELETE"]);
        // Le croisement du filet tombe sur le séparateur des colonnes : un
        // trait décalé d'un signe suffit à défaire le tableau.
        let bar = |line: &String| line.chars().position(|c| c == '│' || c == '┼');
        let column = bar(&rows[0].display_lines[0]);
        assert!(column.is_some());
        assert_eq!(bar(&rows[0].display_lines[1]), column);
        assert_eq!(bar(&rows[1].display_lines[0]), column);
        assert_eq!(bar(&rows[2].display_lines[0]), column);
    }

    #[test]
    fn a_table_left_open_by_the_stream_is_still_delivered() {
        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        builder.push_block("| Route | Méthode |\n|---|---|\n| /users | GET |");
        builder.finish();
        assert_eq!(table_rows(builder.document()).len(), 2);
    }

    #[test]
    fn blockquotes_are_captured() {
        let d = doc("> Une citation utile.\n");
        assert!(d.blocks.iter().any(|b| b.kind == BlockKind::Quote));
        assert!(d.blocks[0].text.contains("citation"));
    }

    #[test]
    fn headings_open_sections_and_carry_levels() {
        let d = doc("# Un\n\nTexte a.\n\n## Deux\n\nTexte b.\n");
        assert_eq!(d.sections.len(), 3); // racine + 2 titres
        assert_eq!(d.sections[1].level, 1);
        assert_eq!(d.sections[2].level, 2);
        assert_eq!(d.sections[1].title, "Un");
        // Le texte suit bien sa section.
        let b = d.blocks.iter().find(|b| b.text == "Texte b.").unwrap();
        assert_eq!(b.section_id, 2);
    }

    #[test]
    fn a_numbered_heading_shows_its_number_but_says_it_in_words() {
        let d = parse_document("## 1. Contexte\n\nDu texte.", CodePolicy::Announce);
        assert_eq!(
            d.sections[1].title, "1. Contexte",
            "le sommaire montre le titre tel qu'il est écrit"
        );
        let heading = &d.sentences[0];
        assert_eq!(heading.text, "un, Contexte", "la voix dit le rang");
        let shown: Vec<&str> = heading.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(shown, ["1.", "Contexte"], "l'écran montre le chiffre");
    }

    #[test]
    fn the_document_title_is_shown_as_written() {
        let d = parse_document("# 3.2 Passerelle\n\nDu texte.", CodePolicy::Announce);
        assert_eq!(d.title, "3.2 Passerelle");
    }

    #[test]
    fn a_heading_without_a_number_is_left_alone() {
        let d = parse_document("## Contexte\n\nDu texte.", CodePolicy::Announce);
        assert_eq!(d.sections[1].title, "Contexte");
        assert_eq!(d.sentences[0].text, "Contexte");
    }

    #[test]
    fn heading_markers_are_never_pronounced() {
        let d = doc("## Performance & Parallélisme\n");
        assert_eq!(d.blocks[0].text, "Performance & Parallélisme");
        assert!(!d.blocks[0].text.contains('#'));
    }

    #[test]
    fn words_keep_their_punctuation_for_display() {
        let d = doc("Bonjour, monde !\n");
        let words: Vec<&str> = d.sentences[0].words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(words, vec!["Bonjour,", "monde", "!"]);
        assert_eq!(d.sentences[0].words[0].punctuation, Some(Punctuation::Comma));
    }

    #[test]
    fn incremental_and_bulk_parsing_agree() {
        let md = "# Titre\n\nUn.\n\n- a\n- b\n\n## Autre\n\nDeux.\n";
        let bulk = doc(md);

        let mut builder = DocumentBuilder::new(CodePolicy::Announce);
        for block in crate::input::split_blocks(md) {
            builder.push_block(&block);
        }
        let incremental = builder.into_document();

        assert_eq!(bulk.blocks, incremental.blocks);
        assert_eq!(bulk.sentences, incremental.sentences);
        assert_eq!(bulk.sections, incremental.sections);
    }

    #[test]
    fn an_announced_code_block_keeps_its_source_for_the_screen() {
        let doc = parse_document("```rust\nfn a() {\n    b();\n}\n```", CodePolicy::Announce);
        let block = doc
            .blocks
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Code { .. }))
            .expect("un bloc de code");
        // La voix n'en dit que l'annonce…
        assert_eq!(doc.sentences[block.sentences.0].text, "Bloc de code rust.");
        // …tandis que l'écran garde le source, indentation comprise.
        assert_eq!(block.display_lines, vec!["fn a() {", "    b();", "}"]);
    }

    #[test]
    fn a_read_code_block_also_keeps_its_untouched_source() {
        let doc = parse_document("```json\n{ \"id\": 42 }\n```", CodePolicy::Read);
        let block = doc
            .blocks
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Code { .. }))
            .expect("un bloc de code");
        // Le nombre est mis en mots pour la voix, jamais pour l'écran.
        assert!(doc.sentences[block.sentences.0].text.contains("quarante-deux"));
        assert_eq!(block.display_lines, vec!["{ \"id\": 42 }"]);
    }

    #[test]
    fn prose_is_shown_with_the_lines_of_the_file() {
        // L'œil suit le document tel qu'il est composé ; la voix n'y voit
        // qu'une phrase, d'un seul tenant.
        let doc = doc("Une première ligne\nune seconde ligne\n");
        assert_eq!(
            doc.blocks[0].display_lines,
            vec!["Une première ligne", "une seconde ligne"]
        );
        assert_eq!(doc.sentences[0].text, "Une première ligne une seconde ligne");
    }

    #[test]
    fn a_word_keeps_the_emphasis_it_carries_in_the_file() {
        let doc = doc(
            "Un mot **en gras**, un autre en *italique*, un [lien](https://exemple.fr) et du `code`.\n",
        );
        let words = &doc.sentences[0].words;
        let emphasis = |wanted: &str| {
            words
                .iter()
                .find(|w| w.text.trim_matches(|c: char| !c.is_alphanumeric()) == wanted)
                .unwrap_or_else(|| panic!("le mot « {wanted} » est absent de {words:?}"))
                .emphasis
        };
        assert!(emphasis("gras").strong, "le gras du fichier se voit à l'écran");
        assert!(emphasis("italique").italic);
        assert!(emphasis("lien").link);
        assert!(emphasis("code").code);
        assert!(emphasis("mot").is_plain(), "la prose reste de la prose");
        // La voix, elle, n'en sait rien : elle dirait le marqueur.
        assert!(doc.sentences[0].text.contains("en gras"));
    }

    #[test]
    fn the_words_shown_and_the_words_read_are_the_same() {
        // L'invariant dont dépend tout le surlignage : la ligne montrée et la
        // phrase dite portent les mêmes mots, dans le même ordre. Sans lui,
        // le mot prononcé se poserait à côté de celui qu'on entend.
        let doc = doc(concat!(
            "## 2. Le **contexte**\n\n",
            "Un paragraphe avec un [lien](https://exemple.fr) et de l'*italique*,\n",
            "coupé sur deux lignes, chiffré à 10 000 euros le 3 mars 2027.\n\n",
            "- Un item de liste\n",
            "- Un second, plus long, qui parle du SIRET 984 561 002 00017\n\n",
            "> Une citation, sur une ligne.\n",
        ));
        assert!(doc.blocks.len() >= 4, "le document doit être varié");
        for block in &doc.blocks {
            let shown: Vec<&str> = block
                .display_lines
                .iter()
                .flat_map(|line| line.split_whitespace())
                .collect();
            let said: Vec<&str> = (block.sentences.0..block.sentences.1)
                .flat_map(|id| doc.sentences[id].words.iter().map(|w| w.text.as_str()))
                .collect();
            assert_eq!(shown, said, "bloc {:?}", block.kind);
        }
    }

    #[test]
    fn the_indentation_of_a_line_survives() {
        // L'alignement d'une colonne porte du sens : l'écraser revient à
        // recomposer le document.
        let doc = doc("Nom      Rôle\nalpha    serveur\n");
        assert_eq!(
            doc.blocks[0].display_lines,
            vec!["Nom      Rôle", "alpha    serveur"]
        );
    }

    #[test]
    fn an_english_document_keeps_its_figures() {
        let d = parse_document_in("The report has 37 pages, dated 2024.\n", CodePolicy::Announce, "en");
        let s = &d.sentences[0];
        assert_eq!(s.text, "The report has 37 pages, dated 2024.");
        assert!(s.words.iter().all(|w| w.spoken.is_none()), "{:?}", s.words);
    }

    #[test]
    fn a_french_document_still_says_its_figures_in_words() {
        let d = parse_document_in("Le rapport a 37 pages.\n", CodePolicy::Announce, "fr");
        assert!(d.sentences[0].text.contains("trente-sept"), "{}", d.sentences[0].text);
        assert_eq!(parse_document("Le rapport a 37 pages.\n", CodePolicy::Announce), d);
    }

    #[test]
    fn an_english_heading_keeps_its_number_as_written() {
        let d = parse_document_in("## 1. Context\n\nSome text.", CodePolicy::Announce, "en");
        assert_eq!(d.sentences[0].text, "1. Context");
    }

    #[test]
    fn code_and_tables_are_announced_in_the_document_language() {
        let d = parse_document_in("```rust\nfn a() {}\n```", CodePolicy::Announce, "en");
        assert_eq!(d.sentences[0].text, "Code block rust.");
        let t = parse_document_in("| Name | Port |\n|---|---|\n| alpha | 8080 |\n", CodePolicy::Announce, "en");
        let spoken: Vec<&str> = t.sentences.iter().map(|s| s.text.as_str()).collect();
        assert!(spoken.contains(&"Name: alpha."), "{spoken:?}");
    }

    #[test]
    fn the_builder_remembers_its_language() {
        assert_eq!(DocumentBuilder::new(CodePolicy::Announce).language(), "fr");
        assert_eq!(DocumentBuilder::with_language(CodePolicy::Skip, "en").language(), "en");
    }
}

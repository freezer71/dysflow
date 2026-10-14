//! Découpage d'un markdown en blocs analysables isolément, au fil du flux
//! comme d'un seul coup.

use std::time::{Duration, Instant};

/// Délai d'inactivité après lequel un bloc incomplet est tout de même livré.
///
/// Sans lui, un flux qui s'interrompt au milieu d'un paragraphe resterait muet.
const IDLE_FLUSH: Duration = Duration::from_millis(400);

/// Délai accordé à un bloc visiblement inachevé — tableau en cours, phrase
/// sans point final.
///
/// Un générateur lent écrit une ligne à la fois : livrée seule, une ligne
/// `| a | b |` n'est pas un tableau et ses barres seraient lues ; un
/// paragraphe coupé en pleine phrase donnerait deux phrases à la voix. On
/// attend un peu plus, sans jamais attendre indéfiniment.
const IDLE_FLUSH_UNFINISHED: Duration = Duration::from_millis(1500);

/// Regroupe des lignes en blocs markdown analysables isolément.
///
/// Un bloc se ferme sur une ligne vide hors clôture de code, à EOF, ou après un
/// silence de [`IDLE_FLUSH`] — ce dernier cas permet de lire un flux qui
/// n'envoie pas de ligne vide finale.
#[derive(Default)]
pub struct BlockAssembler {
    buffer: Vec<String>,
    in_fence: bool,
    fence_marker: Option<String>,
    last_activity: Option<Instant>,
}

impl BlockAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Absorbe une ligne ; renvoie un bloc dès qu'il est complet.
    pub fn push(&mut self, line: &str) -> Option<String> {
        self.push_at(line, Instant::now())
    }

    fn push_at(&mut self, line: &str, now: Instant) -> Option<String> {
        self.last_activity = Some(now);
        let trimmed = line.trim_start();

        if let Some(marker) = fence_marker(trimmed) {
            match &self.fence_marker {
                // Une clôture doit être au moins aussi longue que l'ouverture.
                Some(open) if marker.starts_with(&open[..1]) && marker.len() >= open.len() => {
                    self.buffer.push(line.to_string());
                    self.in_fence = false;
                    self.fence_marker = None;
                    return self.take();
                }
                Some(_) => {}
                None => {
                    self.in_fence = true;
                    self.fence_marker = Some(marker);
                }
            }
        }

        if line.trim().is_empty() && !self.in_fence {
            return self.take();
        }

        self.buffer.push(line.to_string());
        None
    }

    /// Livre le bloc en cours si l'entrée est silencieuse depuis assez longtemps.
    pub fn poll_idle(&mut self) -> Option<String> {
        self.poll_idle_at(Instant::now())
    }

    fn poll_idle_at(&mut self, now: Instant) -> Option<String> {
        if self.in_fence || self.buffer.is_empty() {
            return None;
        }
        let patience = if self.looks_unfinished() { IDLE_FLUSH_UNFINISHED } else { IDLE_FLUSH };
        let idle = self
            .last_activity
            .is_some_and(|t| now.saturating_duration_since(t) >= patience);
        if idle {
            self.take()
        } else {
            None
        }
    }

    /// Vrai quand la dernière ligne annonce une suite : une ligne de tableau,
    /// ou une phrase qui n'a pas trouvé sa ponctuation finale.
    fn looks_unfinished(&self) -> bool {
        let Some(last) = self.buffer.last().map(|l| l.trim_end()) else {
            return false;
        };
        if last.starts_with('|') {
            return true;
        }
        !last.ends_with(['.', '!', '?', ':', '…', ')', '»', '"', '`'])
    }

    /// Vide le tampon, quelle que soit son état — à appeler à EOF.
    pub fn finish(&mut self) -> Option<String> {
        self.in_fence = false;
        self.fence_marker = None;
        self.take()
    }

    fn take(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        let block = std::mem::take(&mut self.buffer).join("\n");
        if block.trim().is_empty() {
            return None;
        }
        Some(block)
    }
}

/// Renvoie le marqueur de clôture (```` ``` ```` ou `~~~`) si la ligne en ouvre une.
fn fence_marker(trimmed: &str) -> Option<String> {
    for symbol in ['`', '~'] {
        let count = trimmed.chars().take_while(|&c| c == symbol).count();
        if count >= 3 {
            return Some(symbol.to_string().repeat(count));
        }
    }
    None
}

/// Découpe un document complet en blocs, comme le ferait le flux.
///
/// Garantit que lecture en une passe et lecture en streaming produisent le même
/// document.
pub fn split_blocks(markdown: &str) -> Vec<String> {
    let mut assembler = BlockAssembler::new();
    let mut blocks = Vec::new();
    for line in markdown.lines() {
        if let Some(block) = assembler.push(line) {
            blocks.push(block);
        }
    }
    if let Some(block) = assembler.finish() {
        blocks.push(block);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_blank_lines() {
        let blocks = split_blocks("un\n\ndeux\n\ntrois");
        assert_eq!(blocks, vec!["un", "deux", "trois"]);
    }

    #[test]
    fn keeps_fenced_code_together_despite_blank_lines() {
        let md = "avant\n\n```rust\nfn a() {}\n\nfn b() {}\n```\n\naprès";
        let blocks = split_blocks(md);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[1].contains("fn a()"));
        assert!(blocks[1].contains("fn b()"), "la ligne vide a coupé le bloc");
    }

    #[test]
    fn keeps_tight_lists_in_one_block() {
        let blocks = split_blocks("- a\n- b\n- c\n");
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn tilde_fences_are_supported() {
        let blocks = split_blocks("~~~\ncode\n\nencore\n~~~\n");
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn finish_flushes_an_unterminated_fence() {
        let mut assembler = BlockAssembler::new();
        assert!(assembler.push("```").is_none());
        assert!(assembler.push("du code").is_none());
        assert!(assembler.finish().is_some());
    }

    #[test]
    fn a_finished_paragraph_is_delivered_after_a_short_pause() {
        let mut assembler = BlockAssembler::new();
        let t0 = Instant::now();
        assembler.push_at("Une phrase entière.", t0);
        assert!(assembler.poll_idle_at(t0 + Duration::from_millis(500)).is_some());
    }

    #[test]
    fn an_open_table_waits_longer_before_being_delivered() {
        // Livrée ligne par ligne, une ligne « | a | b | » seule n'est pas un
        // tableau : ses barres seraient lues.
        let mut assembler = BlockAssembler::new();
        let t0 = Instant::now();
        assembler.push_at("| Route | Auth |", t0);
        assembler.push_at("|---|---|", t0);
        assert!(assembler.poll_idle_at(t0 + Duration::from_millis(500)).is_none());
        assert!(assembler.poll_idle_at(t0 + Duration::from_millis(1600)).is_some());
    }

    #[test]
    fn an_unfinished_sentence_waits_longer_too() {
        let mut assembler = BlockAssembler::new();
        let t0 = Instant::now();
        assembler.push_at("Une phrase coupée au milieu", t0);
        assert!(assembler.poll_idle_at(t0 + Duration::from_millis(500)).is_none());
        assert!(assembler.poll_idle_at(t0 + Duration::from_millis(1600)).is_some());
    }
}

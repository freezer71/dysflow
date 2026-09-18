//! Entrée du lecteur : fichier en argument ou flux sur stdin.
//!
//! Lire stdin jusqu'à EOF avant d'afficher quoi que ce soit interdit de brancher
//! le lecteur sur un générateur lent. Les lignes sont donc remontées au fur et à
//! mesure, et regroupées en blocs markdown par [`BlockAssembler`].

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::{self, Receiver};
use std::thread;
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

/// Message remonté par le lecteur d'entrée.
pub enum InputEvent {
    Line(String),
    Eof,
    Error(String),
}

/// Lit une source ligne par ligne dans un thread dédié.
pub fn spawn_reader<R: Read + Send + 'static>(source: R) -> Receiver<InputEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(source);
        let mut raw = Vec::new();
        let mut first = true;
        loop {
            raw.clear();
            match reader.read_until(b'\n', &mut raw) {
                Ok(0) => break,
                Ok(_) => {}
                Err(err) => {
                    let _ = tx.send(InputEvent::Error(err.to_string()));
                    return;
                }
            }
            let line = decode_line(&raw, first);
            first = false;
            if tx.send(InputEvent::Line(line)).is_err() {
                return; // Le lecteur a quitté.
            }
        }
        let _ = tx.send(InputEvent::Eof);
    });
    rx
}

/// Une ligne de texte, quels que soient ses octets.
///
/// Un `.txt` en latin-1 est courant, et une ligne fautive arrêtait tout le
/// flux : le reste du document était perdu. Un octet qui n'est pas de l'UTF-8
/// devient le caractère de remplacement, et le document se lit. La marque
/// d'ordre des octets qu'un éditeur Windows pose en tête n'est pas du texte,
/// et un retour chariot non plus.
fn decode_line(raw: &[u8], first: bool) -> String {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    if first {
        if let Some(rest) = text.strip_prefix('\u{feff}') {
            text = rest.to_string();
        }
    }
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    text
}

/// Un texte entier, tel que [`decode_line`] l'aurait lu ligne à ligne.
pub fn decode_text(raw: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(raw).into_owned();
    if let Some(rest) = text.strip_prefix('\u{feff}') {
        text = rest.to_string();
    }
    text.replace("\r\n", "\n")
}

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

/// Rend le descripteur 0 au terminal, et renvoie le flux d'origine.
///
/// Quand le contenu arrive par un tube, `stdin` n'est plus un terminal. La
/// bibliothèque de terminal se rabat alors sur `/dev/tty` — et sur macOS,
/// kqueue refuse ce descripteur avec `EINVAL` : le clavier devient muet dans le
/// cas d'usage principal, `cat notes.md | sillon`.
///
/// L'alias `/dev/tty` n'est pas surveillable, mais le périphérique qu'il désigne
/// l'est. On résout donc le vrai chemin (`/dev/ttys001`…) depuis une sortie
/// encore rattachée au terminal, on l'ouvre, et on le branche sur le
/// descripteur 0. La bibliothèque voit alors un `stdin` qui est un terminal
/// surveillable, et le contenu se lit sur le descripteur mis de côté.
///
/// Renvoie `None` si l'entrée est déjà un terminal : il n'y a rien à intervertir.
#[cfg(unix)]
pub fn detach_piped_stdin() -> std::io::Result<Option<std::fs::File>> {
    use std::io::IsTerminal;
    use std::os::fd::{AsRawFd, FromRawFd};

    if std::io::stdin().is_terminal() {
        return Ok(None);
    }

    // SAFETY : duplique le descripteur 0, ouvert par définition.
    let saved = unsafe { libc::dup(libc::STDIN_FILENO) };
    if saved < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY : `saved` est un descripteur neuf dont nous prenons la propriété.
    let piped = unsafe { std::fs::File::from_raw_fd(saved) };

    // Sans terminal de contrôle (cron, service…), on garde le tube tel quel :
    // l'erreur d'interface qui suivra sera plus parlante.
    let Some(tty) = open_controlling_terminal() else {
        return Ok(Some(piped));
    };

    // SAFETY : les deux descripteurs sont valides ; `dup2` ferme l'ancien 0.
    if unsafe { libc::dup2(tty.as_raw_fd(), libc::STDIN_FILENO) } < 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(Some(piped))
}

/// Ouvre le terminal de contrôle par son vrai chemin.
///
/// `ttyname` est interrogé sur la sortie standard puis sur la sortie d'erreur :
/// l'une au moins reste rattachée au terminal quand seule l'entrée est
/// redirigée. `/dev/tty` ne sert que de dernier recours.
#[cfg(unix)]
fn open_controlling_terminal() -> Option<std::fs::File> {
    use std::ffi::CStr;

    let open = |path: &str| {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .ok()
    };

    for fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY : `ttyname` rend un pointeur statique, ou nul si `fd` n'est
        // pas un terminal.
        let raw = unsafe { libc::ttyname(fd) };
        if raw.is_null() {
            continue;
        }
        // SAFETY : pointeur non nul rendu par `ttyname`, terminé par un zéro.
        let path = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        if let Some(file) = open(&path) {
            return Some(file);
        }
    }
    open("/dev/tty")
}

#[cfg(not(unix))]
pub fn detach_piped_stdin() -> std::io::Result<Option<std::fs::File>> {
    Ok(None)
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
    fn reader_streams_lines_then_eof() {
        let rx = spawn_reader(std::io::Cursor::new("a\nb\n"));
        let mut lines = Vec::new();
        loop {
            match rx.recv().unwrap() {
                InputEvent::Line(l) => lines.push(l),
                InputEvent::Eof => break,
                InputEvent::Error(e) => panic!("{e}"),
            }
        }
        assert_eq!(lines, vec!["a", "b"]);
    }


    fn drain(rx: Receiver<InputEvent>) -> (Vec<String>, Option<String>) {
        let mut lines = Vec::new();
        loop {
            match rx.recv().unwrap() {
                InputEvent::Line(l) => lines.push(l),
                InputEvent::Eof => return (lines, None),
                InputEvent::Error(e) => return (lines, Some(e)),
            }
        }
    }

    #[test]
    fn an_invalid_byte_does_not_stop_the_stream() {
        // Un `.txt` en latin-1 est courant ; une ligne fautive arrêtait tout
        // le flux, le reste du document perdu.
        let rx = spawn_reader(std::io::Cursor::new(b"ok\n\xE9t\xE9\napres\n".to_vec()));
        let (lines, error) = drain(rx);
        assert_eq!(error, None);
        assert_eq!(lines, vec!["ok", "\u{FFFD}t\u{FFFD}", "apres"]);
    }

    #[test]
    fn a_byte_order_mark_is_dropped() {
        let rx = spawn_reader(std::io::Cursor::new("\u{feff}# Titre\nsuite\n"));
        assert_eq!(drain(rx).0, vec!["# Titre", "suite"]);
    }

    #[test]
    fn windows_line_endings_are_dropped() {
        let rx = spawn_reader(std::io::Cursor::new("a\r\nb\r\nc"));
        assert_eq!(drain(rx).0, vec!["a", "b", "c"]);
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

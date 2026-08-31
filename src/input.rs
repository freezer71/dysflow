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
        let reader = BufReader::new(source);
        for line in reader.lines() {
            let message = match line {
                Ok(text) => InputEvent::Line(text),
                Err(err) => {
                    let _ = tx.send(InputEvent::Error(err.to_string()));
                    return;
                }
            };
            if tx.send(message).is_err() {
                return; // Le lecteur a quitté.
            }
        }
        let _ = tx.send(InputEvent::Eof);
    });
    rx
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
        self.last_activity = Some(Instant::now());
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
        if self.in_fence || self.buffer.is_empty() {
            return None;
        }
        let idle = self.last_activity.is_some_and(|t| t.elapsed() >= IDLE_FLUSH);
        if idle {
            self.take()
        } else {
            None
        }
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
/// cas d'usage principal, `cat notes.md | reader`.
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
}

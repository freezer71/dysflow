//! Entrée du lecteur : fichier en argument ou flux sur stdin.
//!
//! Lire stdin jusqu'à EOF avant d'afficher quoi que ce soit interdit de brancher
//! le lecteur sur un générateur lent. Les lignes sont donc remontées au fur et à
//! mesure, et regroupées en blocs markdown par `doc::blocks::BlockAssembler`.

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::{self, Receiver};

/// Message remonté par le lecteur d'entrée.
pub enum InputEvent {
    Line(String),
    Eof,
    Error(String),
}

/// Lit une source ligne par ligne dans un thread dédié.
pub fn spawn_reader<R: Read + Send + 'static>(source: R) -> Receiver<InputEvent> {
    let (tx, rx) = mpsc::channel();
    crate::sync::spawn_named("dysflow-entree", move || {
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

/// Rend le descripteur 0 au terminal, et renvoie le flux d'origine.
///
/// Quand le contenu arrive par un tube, `stdin` n'est plus un terminal. La
/// bibliothèque de terminal se rabat alors sur `/dev/tty` — et sur macOS,
/// kqueue refuse ce descripteur avec `EINVAL` : le clavier devient muet dans le
/// cas d'usage principal, `cat notes.md | dysflow`.
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
}

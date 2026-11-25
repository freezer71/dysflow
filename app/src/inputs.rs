//! Ce qui entre dans le lecteur : un fichier (argument, boîte d'ouverture,
//! dépose), le presse-papiers, les touches.
//!
//! `open_path`, `paste` et `paste_with` BLOQUENT : l'ouverture d'un PDF, la
//! reconnaissance d'une image ou du presse-papiers peuvent prendre des
//! secondes. La fenêtre ne les appelle que depuis l'exécuteur d'arrière-plan
//! de GPUI, jamais depuis le fil de l'interface.

use std::fmt::Display;
use std::path::Path;

use dysflow::reader::handle::{reply_channel, Request};
use gpui::{Keystroke, PathPromptOptions};

use crate::keymap::{KeyPress, Modifier};
use crate::session::Session;

/// Les formats que la boîte d'ouverture propose et que la fenêtre accepte.
pub const OPENABLE: [&str; 8] = ["md", "txt", "pdf", "png", "jpg", "jpeg", "tiff", "webp"];

/// Vrai si l'extension du chemin est l'un des formats lisibles (sans
/// égard à la casse). Un dossier ou un fichier sans extension passe : le
/// lecteur reconnaît alors son contenu à ses premiers octets.
///
/// La boîte d'ouverture de GPUI 0.3.14 (`PathPromptOptions`) ne sait pas
/// filtrer par type : le filtre s'applique ici, à chaque entrée.
pub fn openable(path: &Path) -> Result<(), String> {
    let Some(extension) = path.extension() else {
        return Ok(());
    };
    if path.is_dir() {
        return Ok(());
    }
    let extension = extension.to_string_lossy().to_lowercase();
    if OPENABLE.contains(&extension.as_str()) {
        return Ok(());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let formats = OPENABLE.join(", ");
    Err(dysflow::tr!(
        en = "“{}”: this format can't be read (.{}). Readable: {}.",
        fr = "« {} » : ce format ne se lit pas (.{}). Formats lus : {}.",
        name,
        extension,
        formats,
    ))
}

/// Ouvre un fichier dans le lecteur par `Request::OpenFile` et attend sa
/// réponse. BLOQUE jusqu'à ce que le fil du lecteur ait fini d'ouvrir le
/// fichier, sans limite de temps : un long PDF ou une image à reconnaître
/// peuvent demander des minutes. Si le fil du lecteur s'arrête entre-temps,
/// la réponse ne viendra jamais : son canal se ferme et l'attente rend une
/// erreur. Le document courant ne change que si l'ouverture réussit.
pub fn open_path(session: &Session, path: &Path) -> Result<(), String> {
    openable(path)?;
    let (reply, answer) = reply_channel();
    session.send(Request::OpenFile {
        path: path.to_path_buf(),
        reply,
    });
    answer.recv().unwrap_or_else(|_| {
        Err(dysflow::tr!("the reader stopped", "le lecteur s'est arrêté").to_string())
    })
}

/// Lit le presse-papiers du système (`dysflow::source::clipboard`, qui rend
/// déjà du Markdown) et l'ouvre. BLOQUE le temps de lire le presse-papiers,
/// et de reconnaître son image quand l'app est construite avec l'OCR.
pub fn paste(session: &Session) -> Result<(), String> {
    paste_with(session, dysflow::source::clipboard)
}

/// Comme `paste`, avec le presse-papiers donné par `read` (les essais n'en
/// lisent jamais de vrai). Un presse-papiers vide ou illisible laisse le
/// document courant ; sinon le texte part au lecteur par `Request::OpenText`.
pub fn paste_with<E: Display>(
    session: &Session,
    read: impl FnOnce() -> Result<String, E>,
) -> Result<(), String> {
    let markdown = read().map_err(|e| format!("{e:#}"))?;
    if markdown.trim().is_empty() {
        return Err(
            dysflow::tr!("The clipboard is empty", "Le presse-papiers est vide").to_string(),
        );
    }
    session.send(Request::OpenText {
        markdown,
        source: "clipboard".into(),
    });
    Ok(())
}

/// La touche de commande du système : ⌘ sur Mac, Ctrl ailleurs.
pub fn command_modifier() -> Modifier {
    if cfg!(target_os = "macos") {
        Modifier::Command
    } else {
        Modifier::Control
    }
}

/// Les signes que `keymap` attend par le caractère produit, et non par la
/// touche : sur un clavier AZERTY, `?` est Maj+`,` et `+` est Maj+`=`.
const PUNCTUATION: [&str; 5] = ["+", "-", "?", "/", ","];

/// La touche de GPUI telle que `keymap` l'attend.
///
/// Pour la ponctuation, c'est le caractère réellement produit
/// (`key_char`) qui compte, quand GPUI le donne : Maj a alors servi à le
/// produire et n'est plus un modificateur (sur AZERTY, Maj+`:` donne `/`,
/// qui est la recherche, pas l'aide). Sinon, la touche (`key`) et Maj tels
/// quels : `n` avec Maj reste ⇧N.
pub fn key_press(keystroke: &Keystroke) -> KeyPress {
    let m = keystroke.modifiers;
    let produced = keystroke
        .key_char
        .as_deref()
        .filter(|c| PUNCTUATION.contains(c));
    let (key, shift) = match produced {
        Some(c) if c != keystroke.key => (c.to_string(), false),
        _ => (keystroke.key.clone(), m.shift),
    };
    KeyPress {
        key,
        shift,
        alt: m.alt,
        control: m.control,
        platform: m.platform,
    }
}

/// La boîte d'ouverture du système : un seul fichier.
pub fn prompt_options() -> PathPromptOptions {
    PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(dysflow::tr!("Open", "Ouvrir").into()),
    }
}

/// Une touche avec la commande du système, telle qu'elle s'écrit : « ⌘O »
/// sur Mac, « Ctrl+O » ailleurs.
pub fn command_label(key: &str) -> String {
    match command_modifier() {
        Modifier::Command => format!("⌘{}", key.to_uppercase()),
        Modifier::Control => format!("Ctrl+{}", key.to_uppercase()),
    }
}

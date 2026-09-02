//! Adaptation des événements egui aux touches neutres du contrôleur.
//!
//! Les lettres arrivent en texte, avec leur casse — c'est ce que le
//! contrôleur attend ; les touches spéciales arrivent en touches. Ce qu'une
//! fenêtre fait autrement qu'un terminal — défiler d'une page en pixels,
//! ouvrir un fichier, coller, donner le clavier au champ de recherche — remonte
//! en demandes que la fenêtre sert elle-même.

use eframe::egui::{self, Event, Key as EguiKey};

use crate::app::keys::{self, Key, KeyCode};
use crate::app::App;

/// Ce que la trame demande à la fenêtre, en plus du contrôleur.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    /// Pages à défiler, en signe.
    pub page: i32,
    pub open_file: bool,
    pub paste: bool,
    pub focus_search: bool,
    /// La barre latérale — sommaire, signets — se replie ou se déplie.
    pub toggle_sidebar: bool,
    /// Le panneau de note s'ouvre ou se ferme.
    pub toggle_notes: bool,
    /// Échap pressé alors que rien n'était ouvert : la fenêtre décide.
    pub escape: bool,
}

/// Distribue les touches de la trame, sauf quand un champ de saisie a le clavier.
pub fn dispatch(ctx: &egui::Context, app: &mut App) -> Requests {
    let mut requests = Requests::default();
    // Un champ egui a le clavier : ce qu'on tape lui revient, tout entier.
    // C'est egui qui le dit, pas une liste de touches à exclure.
    if ctx.memory(|m| m.focused().is_some()) {
        return requests;
    }
    let events = ctx.input(|i| i.events.clone());
    for event in events {
        match event {
            Event::Paste(_) => requests.paste = true,
            Event::Key { key, pressed: true, modifiers, .. } => {
                if modifiers.command {
                    match key {
                        EguiKey::O => requests.open_file = true,
                        EguiKey::V => requests.paste = true,
                        EguiKey::Q | EguiKey::W => app.should_quit = true,
                        _ => {}
                    }
                    continue;
                }
                match key {
                    EguiKey::PageUp => requests.page -= 1,
                    EguiKey::PageDown => requests.page += 1,
                    // Au terminal, Échap quitte ; dans une fenêtre, on ferme
                    // ce qui est ouvert, et sinon rien — `q` quitte.
                    EguiKey::Escape if !app.show_help && !app.show_config => {
                        requests.escape = true;
                    }
                    // Au terminal, Tab ouvre le panneau du sommaire ; ici la
                    // barre latérale est à la fenêtre.
                    EguiKey::Tab if !app.show_config => requests.toggle_sidebar = true,
                    _ => {
                        if let Some(code) = special(key) {
                            keys::handle(
                                app,
                                Key { code, shift: modifiers.shift, ctrl: modifiers.ctrl },
                            );
                        } else if modifiers.ctrl {
                            if let Some(c) = letter(key) {
                                keys::handle(
                                    app,
                                    Key { code: KeyCode::Char(c), shift: false, ctrl: true },
                                );
                            }
                        }
                    }
                }
            }
            Event::Text(text) => {
                for c in text.chars() {
                    match c {
                        // L'espace est déjà passé en touche.
                        ' ' => {}
                        // Le champ de recherche prend le clavier lui-même.
                        '/' if !app.show_config => requests.focus_search = true,
                        // Sommaire et signets vivent dans la barre latérale,
                        // la note dans son panneau : à la fenêtre, pas au
                        // contrôleur.
                        't' | 'B' if !app.show_config && !app.show_help => {
                            requests.toggle_sidebar = true;
                        }
                        'e' if !app.show_config && !app.show_help => requests.toggle_notes = true,
                        _ => keys::handle(
                            app,
                            Key { code: KeyCode::Char(c), shift: c.is_uppercase(), ctrl: false },
                        ),
                    }
                }
            }
            _ => {}
        }
    }
    requests
}

fn special(key: EguiKey) -> Option<KeyCode> {
    Some(match key {
        EguiKey::ArrowLeft => KeyCode::Left,
        EguiKey::ArrowRight => KeyCode::Right,
        EguiKey::ArrowUp => KeyCode::Up,
        EguiKey::ArrowDown => KeyCode::Down,
        EguiKey::Enter => KeyCode::Enter,
        EguiKey::Escape => KeyCode::Esc,
        EguiKey::Backspace => KeyCode::Backspace,
        EguiKey::Tab => KeyCode::Tab,
        EguiKey::Home => KeyCode::Home,
        EguiKey::End => KeyCode::End,
        EguiKey::Delete => KeyCode::Delete,
        EguiKey::Space => KeyCode::Char(' '),
        _ => return None,
    })
}

/// La lettre d'une touche, pour les raccourcis avec Ctrl.
fn letter(key: EguiKey) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

//! Adaptation des touches crossterm aux touches neutres du contrôleur.
//!
//! La logique — qui fait quoi — vit dans `app::keys` ; ici on ne fait que
//! traduire le vocabulaire du terminal.

use crossterm::event::{KeyCode as Ct, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::keys::{self, Key, KeyCode};
use crate::app::App;
pub use crate::app::keys::{config_help, help};

pub fn handle(app: &mut App, key: KeyEvent) {
    // Sous Windows, chaque touche émet aussi un événement de relâchement.
    if key.kind == KeyEventKind::Release {
        return;
    }
    if let Some(key) = convert(key) {
        keys::handle(app, key);
    }
}

/// Traduit une touche crossterm, ou `None` si le contrôleur n'en a pas l'usage.
pub fn convert(key: KeyEvent) -> Option<Key> {
    let code = match key.code {
        Ct::Char(c) => KeyCode::Char(c),
        Ct::Left => KeyCode::Left,
        Ct::Right => KeyCode::Right,
        Ct::Up => KeyCode::Up,
        Ct::Down => KeyCode::Down,
        Ct::Enter => KeyCode::Enter,
        Ct::Esc => KeyCode::Esc,
        Ct::Backspace => KeyCode::Backspace,
        Ct::Tab => KeyCode::Tab,
        Ct::BackTab => KeyCode::BackTab,
        Ct::Home => KeyCode::Home,
        Ct::End => KeyCode::End,
        Ct::PageUp => KeyCode::PageUp,
        Ct::PageDown => KeyCode::PageDown,
        Ct::Delete => KeyCode::Delete,
        _ => return None,
    };
    Some(Key {
        code,
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shifted_arrow_keeps_its_shift() {
        let key = convert(KeyEvent::new(Ct::Right, KeyModifiers::SHIFT)).unwrap();
        assert_eq!(key.code, KeyCode::Right);
        assert!(key.shift);
        assert!(!key.ctrl);
    }

    #[test]
    fn a_function_key_has_no_meaning_here() {
        assert!(convert(KeyEvent::new(Ct::F(5), KeyModifiers::NONE)).is_none());
    }
}

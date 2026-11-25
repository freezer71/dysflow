//! Les touches du lecteur : la table de la spec (1.5), rien d'autre.
//!
//! Logique pure, sans GPUI : `KeyPress` décrit la touche telle que GPUI la
//! donne (`"space"`, `"left"`, `"n"` avec `shift`…).

use dysflow::reader::command::Command;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyPress {
    /// Le nom de touche de GPUI : `"space"`, `"left"`, `"home"`, `"n"`…
    pub key: String,
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    /// ⌘ sur Mac.
    pub platform: bool,
}

/// La touche de commande du système.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// ⌘, sur Mac.
    Command,
    /// Ctrl, ailleurs.
    Control,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Reader(Command),
    /// Une ligne affichée plus haut ou plus bas : seule la vue sait où elle
    /// commence.
    Line(isize),
    /// Échap : la saisie se valide, l'aide, la recherche ou le sommaire se
    /// ferment, sinon le lecteur.
    Escape,
    Close,
    Copy,
    Paste,
    OpenFile,
    OpenSearch,
    ToggleContents,
    ShowHelp,
    OpenSettings,
    Edit,
    Follow,
}

pub fn action(key: &KeyPress, command: Modifier) -> Option<Action> {
    if key.alt || (key.platform && key.control) {
        return None;
    }
    let with_command = match command {
        Modifier::Command => key.platform,
        Modifier::Control => key.control,
    };
    // Ctrl sous Command, ⌘ sous Control : ce n'est pas la commande d'ici.
    if key.platform || key.control {
        return if with_command {
            command_action(&key.key)
        } else {
            None
        };
    }
    plain_action(key)
}

/// La commande du système plus une touche (Maj ne compte pas, comme avant).
fn command_action(key: &str) -> Option<Action> {
    match key {
        "c" => Some(Action::Copy),
        "v" => Some(Action::Paste),
        "o" => Some(Action::OpenFile),
        "f" => Some(Action::OpenSearch),
        "w" => Some(Action::Close),
        "," => Some(Action::OpenSettings),
        _ => None,
    }
}

fn plain_action(key: &KeyPress) -> Option<Action> {
    let reader = |command| Some(Action::Reader(command));
    match key.key.as_str() {
        "space" => return reader(Command::TogglePlay),
        "escape" => return Some(Action::Escape),
        "home" => return reader(Command::Start),
        "left" | "right" | "up" | "down" if key.shift => return None,
        "left" => return reader(Command::Sentences(-1)),
        "right" => return reader(Command::Sentences(1)),
        "up" => return Some(Action::Line(-1)),
        "down" => return Some(Action::Line(1)),
        _ => {}
    }
    match key.key.as_str() {
        // GPUI donne la lettre minuscule avec Maj ; la majuscule est
        // acceptée aussi.
        "n" if key.shift => reader(Command::SearchAdvance(1)),
        "p" if key.shift => reader(Command::SearchAdvance(-1)),
        "N" => reader(Command::SearchAdvance(1)),
        "P" => reader(Command::SearchAdvance(-1)),
        "n" => reader(Command::Sections(1)),
        "p" => reader(Command::Sections(-1)),
        "+" => reader(Command::ChangeSpeed(0.1)),
        "-" => reader(Command::ChangeSpeed(-0.1)),
        // Sur un clavier américain, ? est Maj+/.
        "/" if key.shift => Some(Action::ShowHelp),
        "/" => Some(Action::OpenSearch),
        "?" => Some(Action::ShowHelp),
        "t" => Some(Action::ToggleContents),
        "e" => Some(Action::Edit),
        "f" => Some(Action::Follow),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> KeyPress {
        KeyPress {
            key: key.to_string(),
            ..KeyPress::default()
        }
    }

    fn shift(key: &str) -> KeyPress {
        KeyPress {
            shift: true,
            ..press(key)
        }
    }

    fn ctrl(key: &str) -> KeyPress {
        KeyPress {
            control: true,
            ..press(key)
        }
    }

    fn cmd(key: &str) -> KeyPress {
        KeyPress {
            platform: true,
            ..press(key)
        }
    }

    fn plain(key: &str) -> Option<Action> {
        action(&press(key), Modifier::Command)
    }

    fn reader(command: Command) -> Option<Action> {
        Some(Action::Reader(command))
    }

    #[test]
    fn space_toggles_play() {
        assert_eq!(plain("space"), reader(Command::TogglePlay));
    }

    #[test]
    fn arrows() {
        assert_eq!(plain("left"), reader(Command::Sentences(-1)));
        assert_eq!(plain("right"), reader(Command::Sentences(1)));
        assert_eq!(plain("up"), Some(Action::Line(-1)));
        assert_eq!(plain("down"), Some(Action::Line(1)));
        for key in ["left", "right", "up", "down"] {
            assert_eq!(action(&shift(key), Modifier::Command), None, "Maj+{key}");
        }
    }

    #[test]
    fn home_starts() {
        assert_eq!(plain("home"), reader(Command::Start));
    }

    #[test]
    fn escape_escapes() {
        assert_eq!(plain("escape"), Some(Action::Escape));
    }

    #[test]
    fn letters() {
        assert_eq!(plain("n"), reader(Command::Sections(1)));
        assert_eq!(plain("p"), reader(Command::Sections(-1)));
        // GPUI donne la lettre minuscule avec Maj ; on accepte aussi la
        // majuscule.
        assert_eq!(
            action(&shift("n"), Modifier::Command),
            reader(Command::SearchAdvance(1))
        );
        assert_eq!(
            action(&shift("p"), Modifier::Command),
            reader(Command::SearchAdvance(-1))
        );
        assert_eq!(plain("N"), reader(Command::SearchAdvance(1)));
        assert_eq!(plain("P"), reader(Command::SearchAdvance(-1)));
        assert_eq!(
            action(&shift("N"), Modifier::Command),
            reader(Command::SearchAdvance(1))
        );
        // + arrive avec ou sans Maj selon le clavier.
        assert_eq!(plain("+"), reader(Command::ChangeSpeed(0.1)));
        assert_eq!(
            action(&shift("+"), Modifier::Command),
            reader(Command::ChangeSpeed(0.1))
        );
        assert_eq!(plain("-"), reader(Command::ChangeSpeed(-0.1)));
        assert_eq!(plain("/"), Some(Action::OpenSearch));
        assert_eq!(plain("t"), Some(Action::ToggleContents));
        assert_eq!(plain("e"), Some(Action::Edit));
        assert_eq!(plain("?"), Some(Action::ShowHelp));
        assert_eq!(
            action(&shift("?"), Modifier::Command),
            Some(Action::ShowHelp)
        );
        assert_eq!(
            action(&shift("/"), Modifier::Command),
            Some(Action::ShowHelp)
        );
        assert_eq!(plain("f"), Some(Action::Follow));
        assert_eq!(plain("z"), None);
    }

    #[test]
    fn command_keys_follow_the_platform() {
        let table = [
            ("c", Action::Copy),
            ("v", Action::Paste),
            ("o", Action::OpenFile),
            ("f", Action::OpenSearch),
            ("w", Action::Close),
            (",", Action::OpenSettings),
        ];
        for (key, expected) in table {
            assert_eq!(
                action(&ctrl(key), Modifier::Control),
                Some(expected.clone()),
                "Ctrl+{key}"
            );
            assert_eq!(
                action(&cmd(key), Modifier::Control),
                None,
                "⌘{key} sous Control"
            );
            assert_eq!(
                action(&cmd(key), Modifier::Command),
                Some(expected),
                "⌘{key}"
            );
            assert_eq!(
                action(&ctrl(key), Modifier::Command),
                None,
                "Ctrl+{key} sous Command"
            );
        }
    }

    #[test]
    fn command_m_is_gone() {
        assert_eq!(action(&cmd("m"), Modifier::Command), None);
        assert_eq!(action(&ctrl("m"), Modifier::Control), None);
    }

    #[test]
    fn alt_or_extra_modifiers_do_nothing() {
        for key in ["space", "left", "home", "n", "+", "/", "c"] {
            let alt = KeyPress {
                alt: true,
                ..press(key)
            };
            assert_eq!(action(&alt, Modifier::Command), None, "⌥{key}");
            let both = KeyPress {
                platform: true,
                control: true,
                ..press(key)
            };
            assert_eq!(action(&both, Modifier::Command), None, "⌘+Ctrl+{key}");
            assert_eq!(action(&both, Modifier::Control), None, "⌘+Ctrl+{key}");
            let alt_cmd = KeyPress {
                alt: true,
                platform: true,
                ..press(key)
            };
            assert_eq!(action(&alt_cmd, Modifier::Command), None, "⌥⌘{key}");
        }
        // La commande de l'autre système n'est pas la commande d'ici.
        assert_eq!(action(&cmd("space"), Modifier::Control), None);
        assert_eq!(action(&ctrl("space"), Modifier::Command), None);
    }
}

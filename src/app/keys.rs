//! Correspondance touches → commandes, la même pour tous les écrans.
//!
//! Deux contextes : la page des réglages, puis la lecture. Chacun consomme
//! les touches qui lui reviennent.

use super::{App, Panel};

const VIEW_STEP: isize = 4;

/// Aide de la vue de lecture : toutes les touches qui y agissent, et
/// seulement elles, dans la langue de l'interface.
pub fn help() -> &'static [(&'static str, &'static str)] {
    match crate::i18n::ui_lang() {
        crate::i18n::UiLang::En => HELP_EN,
        crate::i18n::UiLang::Fr => HELP_FR,
    }
}

/// Aide de la page des réglages : toutes les touches qui y agissent.
pub fn config_help() -> &'static [(&'static str, &'static str)] {
    match crate::i18n::ui_lang() {
        crate::i18n::UiLang::En => CONFIG_HELP_EN,
        crate::i18n::UiLang::Fr => CONFIG_HELP_FR,
    }
}

const HELP_FR: &[(&str, &str)] = &[
    ("Espace", "Lecture / pause"),
    ("← →", "Phrase précédente / suivante"),
    ("↑ ↓", "Bloc précédent / suivant"),
    ("n / p", "Titre suivant / précédent"),
    ("Début", "Début du document"),
    ("/", "Chercher dans le document"),
    ("N / P", "Résultat suivant / précédent"),
    ("+ / -", "Vitesse (enregistrée)"),
    ("t", "Sommaire : ↑ ↓ puis Entrée"),
    ("s", "Réglages"),
    ("Pg↑ Pg↓", "Relire sans déplacer la lecture"),
    ("f", "Reprendre le suivi"),
    ("?", "Aide"),
    ("Échap", "Fermer le sommaire, sinon quitter"),
    ("q", "Quitter"),
];

const HELP_EN: &[(&str, &str)] = &[
    ("Space", "Play / pause"),
    ("← →", "Previous / next sentence"),
    ("↑ ↓", "Previous / next block"),
    ("n / p", "Next / previous heading"),
    ("Home", "Start of the document"),
    ("/", "Search the document"),
    ("N / P", "Next / previous result"),
    ("+ / -", "Speed (saved)"),
    ("t", "Contents: ↑ ↓ then Enter"),
    ("s", "Settings"),
    ("Pg↑ Pg↓", "Scroll back without moving the reading position"),
    ("f", "Follow the reading again"),
    ("?", "Help"),
    ("Esc", "Close the contents, else quit"),
    ("q", "Quit"),
];

const CONFIG_HELP_FR: &[(&str, &str)] = &[
    ("↑ ↓", "Choisir un réglage"),
    ("← → · Entrée", "Changer sa valeur, enregistrée aussitôt"),
    ("t", "Écouter la voix"),
    ("q · Échap · s", "Fermer les réglages"),
    ("?", "Aide"),
];

const CONFIG_HELP_EN: &[(&str, &str)] = &[
    ("↑ ↓", "Choose a setting"),
    ("← → · Enter", "Change its value, saved at once"),
    ("t", "Listen to the voice"),
    ("q · Esc · s", "Close the settings"),
    ("?", "Help"),
];

/// Une touche, sans le vocabulaire d'aucune bibliothèque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub shift: bool,
    pub ctrl: bool,
}

impl Key {
    pub fn plain(code: KeyCode) -> Self {
        Self { code, shift: false, ctrl: false }
    }

    pub fn char(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }
}

pub fn handle(app: &mut App, key: Key) {
    if key.ctrl && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }

    // L'aide se referme sur n'importe quelle touche.
    if app.show_help {
        app.show_help = false;
        return;
    }

    if app.show_config {
        handle_config(app, key);
    } else {
        handle_reader(app, key);
    }
}

fn handle_config(app: &mut App, key: Key) {
    match key.code {
        // Fermer les réglages ne quitte jamais une lecture.
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => app.close_config(),
        KeyCode::Up => app.config_move_row(-1),
        KeyCode::Down => app.config_move_row(1),
        KeyCode::Right | KeyCode::Enter => app.config_adjust_and_save(1),
        KeyCode::Left => app.config_adjust_and_save(-1),
        KeyCode::Char('t') => app.preview_voice(),
        KeyCode::Char('?') => app.show_help = true,
        _ => {}
    }
}

fn handle_reader(app: &mut App, key: Key) {
    // La saisie d'une recherche capte tout : sinon « q » quitterait au lieu de s'écrire.
    if app.search_input().is_some() {
        match key.code {
            KeyCode::Enter => app.search_commit(),
            KeyCode::Esc => app.search_cancel(),
            KeyCode::Backspace => app.search_backspace(),
            KeyCode::Char(c) => app.search_push(c),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            // Échap referme d'abord le sommaire ; rien d'ouvert, il quitte.
            if app.panel != Panel::None {
                app.panel = Panel::None;
            } else {
                app.should_quit = true;
            }
        }

        KeyCode::Char(' ') => app.toggle_play(),

        KeyCode::Right => app.seek_sentences(1),
        KeyCode::Left => app.seek_sentences(-1),

        // Les flèches verticales pilotent le sommaire ouvert, sinon le texte.
        KeyCode::Up => match app.panel {
            Panel::Contents => app.move_selection(-1),
            Panel::None => app.seek_block(-1),
        },
        KeyCode::Down => match app.panel {
            Panel::Contents => app.move_selection(1),
            Panel::None => app.seek_block(1),
        },

        KeyCode::Char('n') => app.seek_section(1),
        KeyCode::Char('p') => app.seek_section(-1),
        KeyCode::Home => app.seek_start(),

        KeyCode::Char('+') => app.nudge_speed(0.1),
        KeyCode::Char('-') => app.nudge_speed(-0.1),

        KeyCode::Char('t') => app.toggle_panel(Panel::Contents),
        KeyCode::Char('s') => app.open_config(),
        KeyCode::Char('f') => app.toggle_follow(),

        // Défiler fige la vue : on relit sans interrompre la voix.
        KeyCode::PageUp => app.scroll_view(-VIEW_STEP),
        KeyCode::PageDown => app.scroll_view(VIEW_STEP),

        KeyCode::Char('/') => app.search_open(),
        KeyCode::Char('N') => app.search_advance(1),
        KeyCode::Char('P') => app.search_advance(-1),

        KeyCode::Enter => app.activate_selection(),
        KeyCode::Char('?') => app.show_help = true,

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::config::Field;
    use crate::app::tests::{app_reading, DOC};
    use std::time::Duration;

    fn press(app: &mut App, code: KeyCode) {
        handle(app, Key::plain(code));
    }

    #[test]
    fn q_and_escape_close_the_settings_without_quitting() {
        for code in [KeyCode::Char('q'), KeyCode::Esc, KeyCode::Char('s')] {
            let mut app = app_reading(DOC);
            press(&mut app, KeyCode::Char('s'));
            assert!(app.show_config, "s ouvre les réglages");
            press(&mut app, code);
            assert!(!app.show_config, "{code:?} ferme les réglages");
            assert!(!app.should_quit, "{code:?} ne quitte pas");
        }
    }

    #[test]
    fn closing_the_settings_with_nothing_to_read_quits() {
        // `dysflow --config` seul : derrière la page, il n'y a rien.
        let mut app = app_reading("");
        app.open_config();
        press(&mut app, KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn w_writes_nothing_any_more_in_the_settings() {
        let mut app = app_reading(DOC);
        app.open_config();
        press(&mut app, KeyCode::Char('w'));
        assert!(app.status().is_empty(), "w a écrit : {:?}", app.status());
    }

    #[test]
    fn the_arrows_of_the_settings_write_at_once() {
        let (dir, path) = crate::app::tests::autosave_target("fleches");
        let mut app = app_reading(DOC);
        app.set_config_target(Some(path.clone()));
        app.open_config();
        app.screen.select(Field::LineSpacing);
        let before = app.config.reading.line_spacing;
        press(&mut app, KeyCode::Right);
        let after_right = crate::config::Config::read_from(&path).unwrap().reading.line_spacing;
        press(&mut app, KeyCode::Enter);
        let after_enter = crate::config::Config::read_from(&path).unwrap().reading.line_spacing;
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(after_right, before + 1, "→ écrit");
        assert_eq!(after_enter, before + 2, "Entrée avance d'un cran et écrit");
    }

    #[test]
    fn the_settings_help_lists_only_what_acts_there() {
        let keys: Vec<&str> = config_help().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["↑ ↓", "← → · Entrée", "t", "q · Échap · s", "?"]);
    }

    #[test]
    fn the_left_arrow_steps_back_even_in_the_middle_of_a_sentence() {
        let mut app = app_reading(DOC);
        app.seek_to(2);
        // On revient en arrière justement quand la phrase est bien engagée :
        // c'est en l'écoutant qu'on s'aperçoit d'avoir manqué la précédente.
        app.player.simulate(2, Duration::from_secs(3), Duration::from_secs(4));

        press(&mut app, KeyCode::Left);

        assert_eq!(app.current_sentence(), 1, "la flèche a relu la phrase courante");
    }

    #[test]
    fn slash_opens_the_search_prompt() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.search_input(), Some(""));
    }

    #[test]
    fn typing_a_query_never_reaches_the_player() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        // « q » quitte en lecture : pendant une saisie, il ne doit
        // qu'écrire.
        for c in "tq".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(app.search_input(), Some("tq"));
        assert_eq!(app.panel, Panel::None, "le sommaire s'est ouvert pendant la saisie");
        assert!(!app.should_quit, "le lecteur a quitté pendant la saisie");
    }

    #[test]
    fn bookmark_keys_do_nothing_any_more() {
        let mut app = app_reading(DOC);
        for c in ['b', 'B', 'd'] {
            press(&mut app, KeyCode::Char(c));
            assert_eq!(app.panel, Panel::None, "{c} a ouvert un panneau");
            assert!(app.status().is_empty(), "{c} a parlé : {:?}", app.status());
        }
    }

    #[test]
    fn enter_commits_the_search_and_escape_abandons_it() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        for c in "deuxieme".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.search_input().is_none());
        assert!(app.search().is_some());

        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::Esc);
        assert!(app.search_input().is_none(), "Échap ferme la saisie");
        assert!(!app.should_quit, "Échap ne doit pas quitter depuis la saisie");
    }

    #[test]
    fn backspace_erases_the_last_letter() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('b'));
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.search_input(), Some("a"));
    }

    #[test]
    fn paging_scrolls_the_view_and_leaves_the_voice_alone() {
        let mut app = app_reading(DOC);
        app.seek_to(app.doc().sentences.len() - 1);
        let reading = app.current_sentence();

        press(&mut app, KeyCode::PageUp);
        assert!(!app.view.is_following(), "la vue devrait s'être figée");
        assert_eq!(app.current_sentence(), reading, "la lecture a suivi la vue");

        press(&mut app, KeyCode::Char('f'));
        assert!(app.view.is_following());
    }

    #[test]
    fn capital_n_walks_through_the_results() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('/'));
        for c in "phrase".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        let first = app.current_sentence();
        handle(&mut app, Key { code: KeyCode::Char('N'), shift: true, ctrl: false });
        assert_ne!(app.current_sentence(), first, "N n'a pas avancé");
    }

    /// Ce qui ne doit pas bouger quand une touche retirée est pressée.
    fn assert_untouched(app: &App, key: &str, volume: f32) {
        assert_eq!(app.current_sentence(), 1, "{key} a déplacé la lecture");
        assert_eq!(app.config.voice.speed, 1.0, "{key} a changé la vitesse");
        assert_eq!(app.player.volume(), volume, "{key} a changé le volume");
        assert!(!app.player.is_muted(), "{key} a coupé le son");
        assert_eq!(app.panel, Panel::None, "{key} a ouvert un panneau");
        assert!(!app.show_help, "{key} a ouvert l'aide");
        assert!(!app.should_quit, "{key} a quitté");
        assert!(app.status().is_empty(), "{key} a parlé : {:?}", app.status());
    }

    #[test]
    fn the_keys_taken_out_do_nothing_any_more() {
        let mut app = app_reading(DOC);
        app.seek_to(1);
        let volume = app.player.volume();
        for c in ['[', ']', 'm', '1', '2', '3', '4', 'g', 'G', 'r', 'w', '=', '_', 'h'] {
            press(&mut app, KeyCode::Char(c));
            assert_untouched(&app, &c.to_string(), volume);
        }
        for code in [KeyCode::End, KeyCode::Tab] {
            press(&mut app, code);
            assert_untouched(&app, &format!("{code:?}"), volume);
        }
    }

    #[test]
    fn home_goes_to_the_start_of_the_document() {
        let mut app = app_reading(DOC);
        app.seek_to(app.doc().sentences.len() - 1);
        press(&mut app, KeyCode::Home);
        assert_eq!(app.current_sentence(), 0);
    }

    #[test]
    fn shift_with_an_arrow_moves_one_sentence_like_the_arrow() {
        let mut app = app_reading(DOC);
        app.config.reading.seek_step = 2;
        handle(&mut app, Key { code: KeyCode::Right, shift: true, ctrl: false });
        assert_eq!(app.current_sentence(), 1, "plus de saut de plusieurs phrases");
    }

    #[test]
    fn escape_closes_the_contents_then_quits() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.panel, Panel::Contents);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.panel, Panel::None, "Échap ferme d'abord le sommaire");
        assert!(!app.should_quit);
        press(&mut app, KeyCode::Esc);
        assert!(app.should_quit, "rien d'ouvert : Échap quitte");
    }

    #[test]
    fn q_quits_even_with_the_contents_open() {
        let mut app = app_reading(DOC);
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn the_help_names_every_reading_key_and_no_other() {
        let keys: Vec<&str> = help().iter().map(|(k, _)| *k).collect();
        for kept in [
            "Espace", "← →", "↑ ↓", "n / p", "Début", "/", "N / P", "+ / -", "t", "s",
            "Pg↑ Pg↓", "f", "?", "Échap", "q",
        ] {
            assert!(keys.contains(&kept), "{kept} manque à l'aide : {keys:?}");
        }
        for gone in [
            "Maj+← →", "g / Début · Fin", "r", "1 2 3 4", "[ / ]", "m", "b", "t / B", "? / h",
            "q / Échap",
        ] {
            assert!(!keys.contains(&gone), "{gone} est encore dans l'aide");
        }
    }

    #[test]
    fn control_c_quits_from_anywhere() {
        let mut app = app_reading(DOC);
        app.show_config = true;
        handle(&mut app, Key { code: KeyCode::Char('c'), shift: false, ctrl: true });
        assert!(app.should_quit);
    }
}

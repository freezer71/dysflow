//! Correspondance touches → commandes, la même pour tous les écrans.
//!
//! Trois contextes se succèdent, du plus captif au plus général : la saisie d'un
//! champ texte, l'écran de configuration, puis la lecture. Chacun consomme les
//! touches qui lui reviennent avant de rendre la main.
//!
//! Le terminal et la fenêtre convertissent leurs événements en `Key` : la
//! logique n'existe qu'une fois, et les deux aides disent la même chose.

use super::config::Page;
use super::{App, Panel};

/// Blocs parcourus par une page de défilement, au terminal.
///
/// Une page entière ferait perdre le fil ; quatre blocs laissent toujours de
/// quoi se raccrocher à ce qu'on vient de lire. La fenêtre, qui connaît sa
/// hauteur, défile en pixels et n'emprunte pas ce chemin.
const VIEW_STEP: isize = 4;

/// Aide de la vue de lecture.
pub const HELP: &[(&str, &str)] = &[
    ("Espace", "Lecture / pause"),
    ("← →", "Phrase précédente / suivante"),
    ("Maj+← →", "Recul / avance rapide"),
    ("↑ ↓", "Bloc précédent / suivant"),
    ("n / p", "Titre suivant / précédent"),
    ("g / Début · Fin", "Début · fin du document"),
    ("r", "Reprendre la phrase"),
    ("+ / -", "Vitesse (resynthèse)"),
    ("1 2 3 4", "Vitesse 0,75× 1× 1,5× 2×"),
    ("[ / ]", "Volume"),
    ("m", "Couper le son"),
    ("b", "Poser / retirer un signet"),
    ("t / B", "Sommaire / signets"),
    ("s", "Configuration et modèles"),
    ("Pg↑ Pg↓", "Défiler sans déplacer la lecture"),
    ("f", "Revenir à la lecture"),
    ("/", "Chercher dans le document"),
    ("N / P", "Résultat suivant / précédent"),
    ("? / h", "Aide"),
    ("q / Échap", "Quitter"),
];

/// Aide de l'écran de configuration.
pub const CONFIG_HELP: &[(&str, &str)] = &[
    ("1 … 5", "Ouvrir une rubrique directement"),
    ("↑ ↓", "Se déplacer dans la liste"),
    ("Entrée", "Ouvrir · éditer · installer · choisir"),
    ("← →", "Modifier une valeur"),
    ("Échap", "Revenir au menu, puis fermer"),
    ("t", "Écouter un aperçu de la voix"),
    ("a", "Montrer les réglages sans effet"),
    ("d", "Supprimer un modèle · annuler"),
    ("w", "Enregistrer la configuration"),
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
    // La saisie d'un champ texte capte tout, sauf sa validation et son abandon.
    if app.screen.editing.is_some() {
        match key.code {
            KeyCode::Enter => app.config_commit_edit(),
            KeyCode::Esc => app.config_cancel_edit(),
            KeyCode::Backspace => app.config_edit_backspace(),
            KeyCode::Char(c) => app.config_edit_push(c),
            _ => {}
        }
        return;
    }

    let in_menu = app.screen.page == Page::Menu;
    match key.code {
        // Échap recule d'un cran : d'une rubrique ouverte on revient au menu,
        // et seulement ensuite on quitte l'écran.
        KeyCode::Esc => {
            if !app.config_back_to_menu() {
                app.toggle_config();
            }
        }
        KeyCode::Char('s') => app.toggle_config(),

        // Un chiffre ouvre directement sa rubrique, depuis n'importe où.
        KeyCode::Char(c @ '1'..='5') => {
            app.config_goto_section(c as usize - '1' as usize);
        }

        KeyCode::Up if in_menu => app.config_move_section(-1),
        KeyCode::Down if in_menu => app.config_move_section(1),
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab if in_menu => app.config_open_section(),
        KeyCode::Left if in_menu => {}

        KeyCode::Up => app.config_move_row(-1),
        KeyCode::Down => app.config_move_row(1),
        KeyCode::Right => app.config_adjust(1),
        KeyCode::Left => app.config_adjust(-1),
        KeyCode::Enter => app.config_activate(),
        KeyCode::Tab | KeyCode::BackTab => {
            app.config_back_to_menu();
        }

        KeyCode::Char('a') => app.config_toggle_show_all(),
        KeyCode::Char('t') => app.preview_voice(),
        KeyCode::Char('d') | KeyCode::Delete => app.config_delete(),
        KeyCode::Char('w') => app.save_config(),
        KeyCode::Char('?') | KeyCode::Char('h') => app.show_help = true,
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

fn handle_reader(app: &mut App, key: Key) {
    // La saisie d'une recherche capte tout : sinon « b » poserait un signet au
    // lieu de s'écrire.
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

    let step = app.config.reading.seek_step as isize;

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            // Échap referme d'abord le panneau ouvert.
            if app.panel != Panel::None {
                app.panel = Panel::None;
            } else {
                app.should_quit = true;
            }
        }

        KeyCode::Char(' ') => app.toggle_play(),

        KeyCode::Right => app.seek_sentences(if key.shift { step } else { 1 }),
        KeyCode::Left => app.seek_sentences(if key.shift { -step } else { -1 }),

        // Les flèches verticales pilotent le panneau ouvert, sinon le texte.
        KeyCode::Up => match app.panel {
            Panel::Contents | Panel::Bookmarks => app.move_selection(-1),
            Panel::None => app.seek_block(-1),
        },
        KeyCode::Down => match app.panel {
            Panel::Contents | Panel::Bookmarks => app.move_selection(1),
            Panel::None => app.seek_block(1),
        },

        KeyCode::Char('n') => app.seek_section(1),
        KeyCode::Char('p') => app.seek_section(-1),
        KeyCode::Home | KeyCode::Char('g') => app.seek_start(),
        KeyCode::End | KeyCode::Char('G') => app.seek_end(),
        KeyCode::Char('r') => app.restart_sentence(),

        KeyCode::Char('+') | KeyCode::Char('=') => app.change_speed(0.1),
        KeyCode::Char('-') | KeyCode::Char('_') => app.change_speed(-0.1),
        KeyCode::Char('1') => app.set_speed(0.75),
        KeyCode::Char('2') => app.set_speed(1.0),
        KeyCode::Char('3') => app.set_speed(1.5),
        KeyCode::Char('4') => app.set_speed(2.0),

        KeyCode::Char(']') => app.change_volume(0.05),
        KeyCode::Char('[') => app.change_volume(-0.05),
        KeyCode::Char('m') => app.toggle_mute(),

        KeyCode::Char('b') => app.toggle_bookmark(),
        KeyCode::Char('B') => app.toggle_panel(Panel::Bookmarks),
        KeyCode::Char('t') | KeyCode::Tab => app.toggle_panel(Panel::Contents),
        KeyCode::Char('s') => app.toggle_config(),
        KeyCode::Char('f') => app.toggle_follow(),

        // Défiler fige la vue : on relit sans interrompre la voix.
        KeyCode::PageUp => app.scroll_view(-VIEW_STEP),
        KeyCode::PageDown => app.scroll_view(VIEW_STEP),

        KeyCode::Char('/') => app.search_open(),
        KeyCode::Char('N') => app.search_advance(1),
        KeyCode::Char('P') => app.search_advance(-1),

        KeyCode::Enter => app.activate_selection(),
        KeyCode::Char('d') => app.delete_selection(),
        KeyCode::Char('w') => app.save_config(),
        KeyCode::Char('?') | KeyCode::Char('h') => app.show_help = true,

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{app_reading, DOC};
    use std::time::Duration;

    fn press(app: &mut App, code: KeyCode) {
        handle(app, Key::plain(code));
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
        // « b » pose un signet en lecture : pendant une saisie, il ne doit
        // qu'écrire.
        for c in "bq".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(app.search_input(), Some("bq"));
        assert!(!app.bookmarked_here(), "un signet a été posé pendant la saisie");
        assert!(!app.should_quit, "le lecteur a quitté pendant la saisie");
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

    #[test]
    fn shift_with_an_arrow_seeks_by_the_configured_step() {
        let mut app = app_reading(DOC);
        app.config.reading.seek_step = 2;
        handle(&mut app, Key { code: KeyCode::Right, shift: true, ctrl: false });
        assert_eq!(app.current_sentence(), 2);
    }

    #[test]
    fn control_c_quits_from_anywhere() {
        let mut app = app_reading(DOC);
        app.show_config = true;
        handle(&mut app, Key { code: KeyCode::Char('c'), shift: false, ctrl: true });
        assert!(app.should_quit);
    }
}

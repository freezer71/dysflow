//! Les gestes de l'interface, traduits en appels au lecteur.

use crate::app::App;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    TogglePlay,
    Pause,
    Words(isize),
    Sentences(isize),
    Blocks(isize),
    Sections(isize),
    SeekTo(usize),
    SeekToWord { sentence: usize, word: usize },
    RestartSentence,
    Start,
    End,
    SetSpeed(f32),
    ChangeSpeed(f32),
    ChangeVolume(f32),
    ToggleMute,
    Search(String),
    SearchAdvance(isize),
    SearchCancel,
}

impl Command {
    pub fn apply(self, app: &mut App) {
        match self {
            Command::TogglePlay => app.toggle_play(),
            Command::Pause => {
                if app.player.is_playing() {
                    app.toggle_play();
                }
            }
            Command::Words(delta) => app.seek_words(delta),
            Command::Sentences(delta) => app.seek_sentences(delta),
            Command::Blocks(delta) => app.seek_block(delta),
            Command::Sections(delta) => app.seek_section(delta),
            Command::SeekTo(sentence) => app.seek_to(sentence),
            Command::SeekToWord { sentence, word } => app.seek_to_word(sentence, word),
            Command::RestartSentence => app.restart_sentence(),
            Command::Start => app.seek_start(),
            Command::End => app.seek_end(),
            Command::SetSpeed(speed) => app.set_speed(speed),
            Command::ChangeSpeed(delta) => app.change_speed(delta),
            Command::ChangeVolume(delta) => app.change_volume(delta),
            Command::ToggleMute => app.toggle_mute(),
            // La recherche du terminal se tape touche par touche ; l'interface
            // la donne d'un bloc.
            Command::Search(query) => {
                app.search_open();
                for c in query.chars() {
                    app.search_push(c);
                }
                app.search_commit();
            }
            Command::SearchAdvance(delta) => app.search_advance(delta),
            Command::SearchCancel => app.search_cancel(),
        }
    }
}

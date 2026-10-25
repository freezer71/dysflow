//! `dysflow` — lecteur audio en terminal pour du texte pipé ou un fichier.

use anyhow::Result;

use dysflow::launch::{self, Launch, Prepared};
use dysflow::tui;

fn main() -> Result<()> {
    // La langue de l'interface se fixe avant tout texte, aide comprise.
    dysflow::i18n::set_ui_lang(dysflow::i18n::UiLang::from_env());
    let args = launch::parse_args();
    let Launch::Ready(prepared) = launch::prepare(&args)? else {
        return Ok(());
    };
    let Prepared { mut app, config_requested, quiet } = *prepared;

    if config_requested {
        app.open_config();
        // Ouverte au lancement, la page passe avant la lecture.
        app.player.pause();
    }
    let outcome = tui::run(app);
    // La garde tombe ici : une erreur remontée par `run` retrouve une sortie
    // d'erreur normale avant d'être affichée.
    drop(quiet);
    outcome
}

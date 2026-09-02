//! `sillon-gui` — le lecteur dans une fenêtre.

use anyhow::Result;
use clap::Parser;

use sillon::launch::{self, Args, Launch, Prepared};

fn main() -> Result<()> {
    let args = Args::parse();
    let Launch::Ready(prepared) = launch::prepare(&args)? else {
        return Ok(());
    };
    let Prepared { mut app, config_requested, has_input: _, quiet } = prepared;
    // Sans document, la fenêtre s'ouvre sur « Ouvrir… », pas sur les réglages :
    // régler avant d'avoir quelque chose à lire n'est pas son geste naturel.
    if config_requested {
        app.open_config();
    }
    let outcome = sillon::gui::run(app);
    drop(quiet);
    outcome
}

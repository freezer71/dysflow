//! Interface plein écran : installation du terminal, boucle d'événements.

pub mod keymap;
pub mod ui;

use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crate::app::App;

/// Cadence de rafraîchissement.
///
/// Le curseur de mots doit rester fluide sans réveiller le processus pour rien ;
/// le rendu ne recalcule de toute façon que la fenêtre visible.
const FRAME: Duration = Duration::from_millis(33);

/// Cadence au repos : rien ne joue, rien n'arrive. Une touche réveille la
/// boucle sur-le-champ ; seul ce qui change sans touche (fin d'un message)
/// attend ce délai.
const IDLE_FRAME: Duration = Duration::from_millis(250);

/// Délai au-delà duquel l'écran se redessine même si sa clé n'a pas bougé :
/// un état que la clé oublierait ne reste pas figé longtemps.
const REFRESH: Duration = Duration::from_millis(500);

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = setup().context("initialisation du terminal")?;
    let result = event_loop(&mut terminal, &mut app);
    // La position se retient même si la boucle a échoué : c'est justement
    // quand quelque chose casse qu'on tient à ne pas tout reprendre.
    app.remember_position();
    // Le terminal est restauré même si la boucle a échoué, et l'erreur de
    // la boucle passe avant celle de la restauration.
    let restored = restore(&mut terminal);
    // La sortie attend la synthèse en cours ; sur un modèle lent, le dire
    // évite de croire à un blocage.
    if let Some(notice) = app.closing_notice() {
        eprintln!("{notice}");
    }
    result.and(restored)
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let mut last_frame = Instant::now();
    // Clé et instant de la dernière trame dessinée. La mise en page du texte
    // coûte : on ne la refait que si ce qui se voit a changé (mot, phrase,
    // message…), après une touche, ou passé `REFRESH`.
    let mut drawn: Option<(u64, Instant)> = None;
    let mut dirty = true;
    while !app.should_quit {
        app.tick();
        let key = app.frame_key();
        let stale = drawn.is_none_or(|(k, at)| k != key || at.elapsed() >= REFRESH);
        if dirty || stale {
            terminal.draw(|frame| ui::render(frame, app))?;
            drawn = Some((key, Instant::now()));
            dirty = false;
        }

        // Consomme les touches en attente sans jamais dépasser une trame : les
        // frappes répétées (avance maintenue) restent réactives.
        let frame = if app.is_idle() { IDLE_FRAME } else { FRAME };
        let budget = frame.saturating_sub(last_frame.elapsed());
        if event::poll(budget)? {
            loop {
                match event::read()? {
                    Event::Key(key) => {
                        keymap::handle(app, key);
                        dirty = true;
                    }
                    Event::Resize(_, _) => dirty = true,
                    _ => {}
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        last_frame = Instant::now();
    }
    Ok(())
}

fn setup() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    // Une panique en mode brut laisserait le terminal inutilisable ; le hook
    // rétablit l'écran avant de propager.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        // Le message doit atteindre le terminal, pas le journal.
        crate::logging::release_for_panic();
        previous(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Pas de capture de la souris : rien ne l'écoute, et elle interdirait la
    // sélection et la copie du texte dans le terminal.
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

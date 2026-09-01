//! Interface plein écran : installation du terminal, boucle d'événements.

pub mod app;
pub mod config;
pub mod keymap;
pub mod search;
pub mod pace;
pub mod startup;
pub mod ui;
pub mod view;

use anyhow::{Context, Result};
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use app::App;

/// Cadence de rafraîchissement.
///
/// Le curseur de mots doit rester fluide sans réveiller le processus pour rien ;
/// le rendu ne recalcule de toute façon que la fenêtre visible.
const FRAME: Duration = Duration::from_millis(33);

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = setup().context("initialisation du terminal")?;
    let result = event_loop(&mut terminal, &mut app);
    // La position se retient même si la boucle a échoué : c'est justement
    // quand quelque chose casse qu'on tient à ne pas tout reprendre.
    app.remember_position();
    // Le terminal est restauré même si la boucle a échoué.
    restore(&mut terminal)?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let mut last_frame = Instant::now();
    while !app.should_quit {
        app.tick();
        terminal.draw(|frame| ui::render(frame, app))?;

        // Consomme les touches en attente sans jamais dépasser une trame : les
        // frappes répétées (avance maintenue) restent réactives.
        let budget = FRAME.saturating_sub(last_frame.elapsed());
        if event::poll(budget)? {
            loop {
                match event::read()? {
                    Event::Key(key) => keymap::handle(app, key),
                    Event::Resize(_, _) => {}
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
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        previous(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

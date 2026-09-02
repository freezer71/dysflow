//! Interface graphique : une fenêtre eframe autour du même contrôleur.
//!
//! Deux modules ne touchent pas à egui et se compilent toujours : `text` met un
//! bloc en runs, `flow` fait l'arithmétique du défilement. C'est là que se
//! vérifie ce que la fenêtre montre. Le reste dessine, et n'existe qu'avec la
//! feature `gui`.

pub mod flow;
pub mod text;

#[cfg(feature = "gui")]
mod fonts;
#[cfg(feature = "gui")]
mod header;
#[cfg(feature = "gui")]
mod help;
#[cfg(feature = "gui")]
mod keys;
#[cfg(feature = "gui")]
mod panel;
#[cfg(feature = "gui")]
mod player;
#[cfg(feature = "gui")]
mod reader;
#[cfg(feature = "gui")]
mod theme;
#[cfg(feature = "gui")]
mod window;

#[cfg(feature = "gui")]
pub use window::run;

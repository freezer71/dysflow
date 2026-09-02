//! Interface graphique : une fenêtre eframe autour du même contrôleur.
//!
//! Trois modules ne touchent pas à egui et se compilent toujours : `text` met
//! un bloc en runs, `flow` fait l'arithmétique du défilement, `timeline` celle
//! de la ligne de temps. C'est là que se vérifie ce que la fenêtre montre. Le
//! reste dessine, et n'existe qu'avec la feature `gui`.

pub mod flow;
pub mod text;
pub mod timeline;

#[cfg(feature = "gui")]
mod fonts;
#[cfg(feature = "gui")]
mod header;
#[cfg(feature = "gui")]
mod help;
#[cfg(feature = "gui")]
mod icons;
#[cfg(feature = "gui")]
mod keys;
#[cfg(feature = "gui")]
mod notes;
#[cfg(feature = "gui")]
mod open;
#[cfg(feature = "gui")]
mod player;
#[cfg(feature = "gui")]
mod reader;
#[cfg(feature = "gui")]
mod settings;
#[cfg(feature = "gui")]
mod sidebar;
#[cfg(all(test, feature = "snapshots"))]
mod snapshot;
#[cfg(feature = "gui")]
mod theme;
#[cfg(feature = "gui")]
mod widgets;
#[cfg(feature = "gui")]
mod window;

#[cfg(feature = "gui")]
pub use window::run;

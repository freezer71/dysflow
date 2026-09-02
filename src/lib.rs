//! Lecteur audio en terminal pour du markdown pipé ou un fichier.
//!
//! L'organisation suit le trajet du texte :
//!
//! ```text
//! entrée ──▶ doc ──▶ tts ──▶ audio ──▶ app ──▶ tui | gui
//! (flux,    (blocs,  (pool    (sortie,  (état,   (rendu,
//!  blocs)   phrases) parallèle) position) commandes) touches)
//! ```

pub mod app;
pub mod audio;
pub mod bookmark;
pub mod config;
pub mod doc;
pub mod gui;
pub mod input;
pub mod launch;
pub mod logging;
pub mod models;
pub mod paths;
pub mod position;
pub mod source;
pub mod text;
pub mod tts;
pub mod tui;

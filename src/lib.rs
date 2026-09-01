//! Lecteur audio en terminal pour du markdown pipé ou un fichier.
//!
//! L'organisation suit le trajet du texte :
//!
//! ```text
//! entrée ──▶ doc ──▶ tts ──▶ audio ──▶ tui
//! (flux)   (blocs,  (pool   (sortie,  (rendu,
//!         phrases) parallèle) position)  contrôles)
//! ```

pub mod audio;
pub mod bookmark;
pub mod config;
pub mod doc;
pub mod input;
pub mod logging;
pub mod models;
pub mod position;
pub mod source;
pub mod text;
pub mod tts;
pub mod tui;

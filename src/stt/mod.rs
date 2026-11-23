//! La dictée : reconnaissance de la parole au terminal.

pub mod detector;
pub mod recognizer;
pub mod segmenter;

#[cfg(feature = "stt-sherpa")]
pub mod sherpa;

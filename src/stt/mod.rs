//! La dictée : reconnaissance de la parole au terminal.

pub mod detector;
pub mod dictation;
pub mod holder;
pub mod recognizer;
pub mod rolling;
pub mod segmenter;
pub mod wer;

#[cfg(feature = "stt-sherpa")]
pub mod sherpa;

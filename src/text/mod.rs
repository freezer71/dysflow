//! Nettoyage et découpage du texte avant mise en voix.

pub mod dates;
pub mod number;
pub mod numerals;
pub mod sanitizer;
pub mod sentence;

pub use sanitizer::sanitize;
pub use sentence::split_sentences;

//! Nettoyage et découpage du texte avant mise en voix.

pub mod acronyms;
pub mod dates;
pub mod language;
pub mod number;
pub mod numerals;
pub mod sanitizer;
pub mod sentence;

pub use sanitizer::{sanitize, sanitize_layout, verbalize};
pub use sentence::split_sentences;

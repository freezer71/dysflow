//! Les briques de NeMo communes aux modèles de dictée MLX (Parakeet, puis
//! Canary, Nemotron, Sortformer) : réglages et couches du transducteur,
//! attention, pas des décodeurs, jetons datés et phrases. Port de
//! `macos/Vendor/KokoroMLX/STT/Nemo/`.

pub mod alignment;
pub mod attention;
pub mod config;
pub mod decoding;
pub mod rnnt;

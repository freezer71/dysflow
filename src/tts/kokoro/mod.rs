//! Kokoro-82M : phonétisation, voix, minutage des mots et moteur. Le calcul
//! passe par `KokoroBackend` (MLX dans l'app, ONNX Runtime au terminal).

pub mod align;
pub mod phonemes;
pub mod voices;

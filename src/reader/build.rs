//! La construction du lecteur de l'app : la configuration lue, la voix, la
//! sortie audio. Partagée par l'interface Swift (`ffi`) et l'app GPUI.

use std::sync::Arc;
use std::time::Instant;

use crate::audio::Player;
use crate::config::Config;
use crate::tts::{self, silent::SilentEngine, TtsEngine};

use super::Reader;

/// Le lecteur de l'app sur la configuration lue (`Config::load`). Un
/// `config.toml` illisible est retenu : la migration ne l'écrasera pas.
/// `compute` : les calculs MLX de l'app ; Kokoro y est enveloppé **une
/// fois** : chaque moteur reçoit ce même `Arc` (voir `SharedCompute`).
pub fn app_reader(loaded: (Config, Option<String>), compute: tts::AppCompute) -> Reader {
    let (config, unreadable) = loaded;
    let available = compute.kokoro.is_some() || compute.text.is_some();
    let mut reader = Reader::new(
        config,
        Box::new(move |config: &Config| {
            tts::build_engine_with(config.engine_preference(), &config.engine_settings(), &compute).0
        }),
        Box::new(Player::with_output),
        Instant::now(),
    );
    reader.set_mlx_compute(available);
    reader.set_config_unreadable(unreadable.is_some());
    reader
}

/// Lecteur muet, sans modèle ni carte son : tests et mode de test. Il ne
/// touche pas au disque de l'utilisateur.
pub fn silent_reader() -> Reader {
    let mut config = Config::default();
    config.reading.autoplay = false;
    config.app.keep_voice_minutes = -1;
    let mut reader = Reader::new(
        config,
        Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
        Box::new(|_: &str| Player::silent()),
        Instant::now(),
    );
    reader.set_persistent(false);
    reader
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_reader_never_autoplays() {
        let reader = silent_reader();
        assert!(!reader.config().reading.autoplay);
        assert_eq!(reader.config().app.keep_voice_minutes, -1);
    }
}

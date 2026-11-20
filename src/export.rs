//! `--output` : le texte rendu en un fichier WAV, sans écran ni lecture.
//!
//! Fait pour être appelé par un programme (un agent, un script) : la voix,
//! la vitesse et la langue sont celles de la configuration, sauf ce que la
//! ligne de commande fixe pour cet appel ; rien n'est réécrit sur le disque.
//! La sortie standard ne reçoit que les octets du WAV (`-o -`) ; tout ce qui
//! se dit (voix retenue, repli, erreurs) passe par la sortie d'erreur.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;

use crate::audio::sonic;
use crate::audio::tempo::{pause_at, MAX_SPEED, MIN_SPEED};
use crate::config::Config;
use crate::tts::{PcmBuffer, TtsParams, TtsSession};

/// Où va le WAV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// `-` : les octets sur la sortie standard.
    Stdout,
    File(PathBuf),
}

impl Destination {
    /// `-` désigne la sortie standard ; un chemin doit finir en `.wav`, ou
    /// n'avoir aucune extension : seul le WAV est produit, et un `.mp3` qui
    /// contiendrait du WAV tromperait celui qui l'ouvre.
    pub fn parse(value: &str) -> Result<Self> {
        if value == "-" {
            return Ok(Destination::Stdout);
        }
        let path = PathBuf::from(crate::paths::expand_tilde(value));
        match path.extension().and_then(|e| e.to_str()) {
            None => Ok(Destination::File(path)),
            Some(ext) if ext.eq_ignore_ascii_case("wav") => Ok(Destination::File(path)),
            Some(ext) => anyhow::bail!(crate::tr!(
                en = "unsupported audio format: .{ext} (only .wav is produced)",
                fr = "format audio non pris en charge : .{ext} (seul le .wav est produit)"
            )),
        }
    }
}

/// Applique la voix demandée pour cet appel.
///
/// Une famille qui nomme ses voix (Kokoro…) range la voix par langue
/// (`voice.per_language`) : `--voice` doit y aller, pour toutes les langues,
/// sinon elle serait ignorée. Une voix inconnue de la famille est une
/// erreur, avec la liste des voix possibles — remplacée en silence par la
/// voix par défaut, elle tromperait celui qui l'a demandée.
pub fn apply_voice(config: &mut Config, requested: Option<&str>) -> Result<()> {
    let Some(requested) = requested.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    let family = config.engine.family.clone();
    let named = config.engine_preference() != crate::tts::EnginePreference::System
        && crate::models::voices_follow_language(&family);
    if !named {
        config.voice.voice = requested.to_string();
        return Ok(());
    }
    let voices = crate::models::voices_for_family(&family);
    if !voices.iter().any(|v| v.id == requested) {
        let known: Vec<&str> = voices.iter().map(|v| v.id).collect();
        anyhow::bail!(crate::tr!(
            en = "unknown voice for {family}: {requested} (available: {}; a system voice needs --engine system)",
            fr = "voix inconnue pour {family} : {requested} (disponibles : {} ; une voix du système demande --engine system)",
            known.join(", ")
        ));
    }
    let mut languages: Vec<String> = config.voice.per_language.keys().cloned().collect();
    languages.extend(["fr".to_string(), "en".to_string()]);
    for language in languages {
        config.voice.per_language.insert(language, requested.to_string());
    }
    Ok(())
}

/// `--list-voices` : les voix que la configuration peut prendre, une par
/// ligne, l'identifiant d'abord (à passer à `--voice`) ; `*` marque celle
/// qui lirait du français et de l'anglais par défaut.
pub fn voice_list(config: &Config) -> String {
    let family = &config.engine.family;
    let system = config.engine_preference() == crate::tts::EnginePreference::System;
    let mut out = String::new();
    if !system && crate::models::voices_follow_language(family) {
        let current: Vec<String> = ["fr", "en"].iter().map(|l| config.tts_params_in(l).voice).collect();
        for voice in crate::models::voices_for_family(family) {
            let mark = if current.iter().any(|c| c == voice.id) { "*" } else { " " };
            out.push_str(&format!("{mark} {}\t{}\n", voice.id, voice.label.get()));
        }
        return out;
    }
    let current = config.voice.voice.trim();
    let mut voices = crate::tts::system_voices::system_voices();
    // `say` cite une voix une fois par variante installée.
    voices.dedup();
    for voice in voices {
        let mark = if voice.name == current { "*" } else { " " };
        out.push_str(&format!("{mark} {}\t{}\n", voice.name, voice.locale));
    }
    out
}

/// Synthétise les phrases dans l'ordre et les met bout à bout, à la vitesse
/// `speed` : la parole passe par Sonic (sans changer la hauteur), la pause
/// de fin de chaque phrase raccourcit comme à la lecture (`pause_at`).
///
/// Une phrase sans texte n'est pas envoyée au moteur ; sa pause compte.
pub fn render<'a>(
    session: &mut dyn TtsSession,
    sentences: impl IntoIterator<Item = (&'a str, u64)>,
    params: &TtsParams,
    speed: f32,
) -> Result<PcmBuffer> {
    let speed = if speed.is_finite() { speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };
    let mut format: Option<(u32, u16)> = None;
    let mut samples: Vec<f32> = Vec::new();
    let mut pending_pause_ms = 0u64;
    for (text, pause_ms) in sentences {
        if !text.trim().is_empty() {
            let pcm = session
                .synthesize(text, params)
                .with_context(|| crate::tr!(en = "synthesizing “{text}”", fr = "synthèse de « {text} »"))?;
            let (rate, channels) = *format.get_or_insert((pcm.sample_rate, pcm.channels));
            if (pcm.sample_rate, pcm.channels) != (rate, channels) {
                anyhow::bail!(crate::tr!(
                    en = "the voice changed format mid-text ({} Hz × {} → {} Hz × {})",
                    fr = "la voix a changé de format en cours de texte ({} Hz × {} → {} Hz × {})",
                    rate, channels, pcm.sample_rate, pcm.channels
                ));
            }
            push_silence(&mut samples, pending_pause_ms, rate, channels, speed);
            pending_pause_ms = 0;
            if (speed - 1.0).abs() < f32::EPSILON {
                samples.extend_from_slice(&pcm.samples);
            } else {
                samples.extend(sonic::stretch(&pcm.samples, rate, channels, speed));
            }
        }
        pending_pause_ms += pause_ms;
    }
    let Some((rate, channels)) = format else {
        anyhow::bail!(crate::tr!("nothing readable", "aucun contenu lisible"));
    };
    // Le silence après la dernière phrase ne sert à rien dans un fichier.
    Ok(PcmBuffer::new(samples, rate, channels))
}

fn push_silence(samples: &mut Vec<f32>, pause_ms: u64, rate: u32, channels: u16, speed: f32) {
    if pause_ms == 0 || samples.is_empty() {
        return;
    }
    let pause = pause_at(std::time::Duration::from_millis(pause_ms), speed);
    let frames = (pause.as_secs_f64() * rate as f64).round() as usize;
    samples.resize(samples.len() + frames * channels as usize, 0.0);
}

/// Le son en WAV 16 bits, le format que tout lecteur ouvre.
pub fn wav_bytes(pcm: &PcmBuffer) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: pcm.channels,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(44 + pcm.samples.len() * 2));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in pcm.samples.iter() {
            let s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
            writer.write_sample((s * i16::MAX as f32).round() as i16)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Écrit `bytes` à destination. Un fichier est écrit d'un coup (`write_atomic`) :
/// celui qui l'attend ne lit jamais un WAV à moitié écrit.
pub fn deliver(bytes: &[u8], destination: &Destination) -> Result<()> {
    match destination {
        Destination::Stdout => {
            let mut out = std::io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()?;
        }
        Destination::File(path) => crate::paths::write_atomic(path, bytes)
            .with_context(|| crate::tr!(en = "writing {}", fr = "écriture de {}", path.display()))?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::TtsParams;

    const RATE: u32 = 24_000;

    /// Une demi-seconde de son par phrase ; retient ce qu'on lui a demandé.
    struct Stub {
        said: Vec<String>,
        rate: u32,
    }

    impl TtsSession for Stub {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            self.said.push(text.to_string());
            let samples = crate::audio::sonic::tests::speech_like(0.5);
            Ok(PcmBuffer::new(samples, self.rate, 1))
        }
    }

    fn stub() -> Stub {
        Stub { said: Vec::new(), rate: RATE }
    }

    #[test]
    fn sentences_are_spoken_in_order_with_their_pauses_between() {
        let mut s = stub();
        let pcm = render(&mut s, [("Un.", 500), ("Deux.", 1000)], &TtsParams::default(), 1.0).unwrap();
        assert_eq!(s.said, ["Un.", "Deux."]);
        // 0,5 s + pause 0,5 s + 0,5 s ; la pause finale n'est pas gardée.
        assert_eq!(pcm.samples.len(), (RATE as f32 * 1.5) as usize);
    }

    #[test]
    fn at_2x_speech_halves_and_the_pause_shrinks_like_in_playback() {
        let mut s = stub();
        let pcm = render(&mut s, [("Un.", 1000), ("Deux.", 0)], &TtsParams::default(), 2.0).unwrap();
        let expected = RATE as f32 * (0.5 + 1.0 / 2f32.sqrt());
        let got = pcm.samples.len() as f32;
        assert!((got - expected).abs() / expected < 0.05, "{got} vs {expected}");
    }

    #[test]
    fn an_empty_sentence_is_not_sent_but_its_pause_counts() {
        let mut s = stub();
        let pcm = render(&mut s, [("Un.", 200), ("  ", 300), ("Deux.", 0)], &TtsParams::default(), 1.0).unwrap();
        assert_eq!(s.said, ["Un.", "Deux."]);
        assert_eq!(pcm.samples.len(), (RATE as f32 * 1.5) as usize);
    }

    #[test]
    fn nothing_to_say_is_an_error() {
        assert!(render(&mut stub(), [("", 100)], &TtsParams::default(), 1.0).is_err());
    }

    #[test]
    fn the_wav_reads_back_with_the_format_and_clamped_samples() {
        let pcm = PcmBuffer::new(vec![0.0, 0.5, 1.5, -2.0], RATE, 1);
        let bytes = wav_bytes(&pcm).unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (RATE, 1, 16));
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, [0, 16384, i16::MAX, -i16::MAX]);
    }

    #[test]
    fn destinations() {
        assert_eq!(Destination::parse("-").unwrap(), Destination::Stdout);
        assert_eq!(Destination::parse("a.wav").unwrap(), Destination::File("a.wav".into()));
        assert_eq!(Destination::parse("a.WAV").unwrap(), Destination::File("a.WAV".into()));
        assert_eq!(Destination::parse("sortie").unwrap(), Destination::File("sortie".into()));
        assert!(Destination::parse("a.mp3").is_err());
    }

    fn kokoro() -> Config {
        let mut c = Config::default();
        c.engine.family = "kokoro".into();
        c.engine.engine = "auto".into();
        c.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        c
    }

    #[test]
    fn a_named_voice_replaces_the_voice_of_every_language() {
        let mut c = kokoro();
        apply_voice(&mut c, Some("af_bella")).unwrap();
        assert_eq!(c.tts_params_in("fr").voice, "af_bella");
        assert_eq!(c.tts_params_in("en").voice, "af_bella");
    }

    #[test]
    fn an_unknown_named_voice_is_refused_with_the_known_ones() {
        let mut c = kokoro();
        let err = apply_voice(&mut c, Some("personne")).unwrap_err().to_string();
        assert!(err.contains("af_bella"), "{err}");
        assert_eq!(c.tts_params_in("fr").voice, "p-fenrir60", "rien n'a changé");
    }

    #[test]
    fn without_a_voice_the_configured_one_stays() {
        let mut c = kokoro();
        apply_voice(&mut c, None).unwrap();
        assert_eq!(c.tts_params_in("fr").voice, "p-fenrir60");
    }

    #[test]
    fn the_system_voice_takes_the_name_as_is() {
        let mut c = kokoro();
        c.engine.engine = "system".into();
        apply_voice(&mut c, Some("Thomas")).unwrap();
        assert_eq!(c.tts_params_in("fr").voice, "Thomas");
    }

    #[test]
    fn the_voice_list_marks_the_default_one() {
        let list = voice_list(&kokoro());
        assert!(list.lines().any(|l| l.starts_with("* p-fenrir60\t")), "{list}");
        assert!(list.lines().any(|l| l.starts_with("  am_adam\t")), "{list}");
    }
}

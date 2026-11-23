//! Parakeet TDT v3 int8 par sherpa-onnx (`nemo_transducer`), sur le
//! processeur : la dictée sous Windows et Linux, et au terminal du Mac
//! jusqu'au port MLX.

use crate::stt::recognizer::Recognizer;
use crate::stt::sherpa::{model_file, same_runtime, VAD_SAMPLE_RATE};
use crate::tts::Cancel;
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Mutex;

/// Les fichiers du paquet `parakeet_tdt_v3_int8`, dans son dossier.
pub const FILES: [&str; 4] = [
    "encoder.int8.onnx",
    "decoder.int8.onnx",
    "joiner.int8.onnx",
    "tokens.txt",
];

/// Parakeet v3 int8. Un énoncé entier en entrée (la découpe aux silences le
/// borne à 12 s), son texte en sortie. Parakeet v3 entend la langue seul :
/// celle de la dictée n'est pas passée au modèle.
///
/// Des contrats de `Recognizer`, `prepare`, `hold` et `memory` gardent leur
/// défaut : `Holder` garde le modèle d'une dictée à l'autre, et sherpa-onnx ne
/// dit pas la mémoire qu'il occupe.
pub struct ParakeetSherpa {
    pub(crate) threads: i32,
    runtime: fn() -> Result<String>,
    recognizer: Mutex<Option<sherpa_onnx::OfflineRecognizer>>,
}

impl ParakeetSherpa {
    /// `threads` : `engine.threads`, au plus 4 (comme Kokoro int8), au moins
    /// 1.
    pub fn new(threads: u32) -> Self {
        Self::with_runtime(threads, same_runtime)
    }

    /// `new`, la vérification d'ONNX Runtime passée en argument : les essais
    /// ne touchent pas à celle, gardée, du processus.
    fn with_runtime(threads: u32, runtime: fn() -> Result<String>) -> Self {
        Self {
            threads: threads.clamp(1, 4) as i32,
            runtime,
            recognizer: Mutex::new(None),
        }
    }
}

impl Recognizer for ParakeetSherpa {
    /// Vérifie d'abord ONNX Runtime (deux copies différentes : refusé avant
    /// de lire le modèle), puis chaque fichier, nommé s'il manque.
    fn load(&self, model_dir: &Path) -> Result<()> {
        (self.runtime)()?;
        let [encoder, decoder, joiner, tokens] =
            FILES.map(|file| model_file(&model_dir.join(file)));
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.transducer = sherpa_onnx::OfflineTransducerModelConfig {
            encoder: Some(encoder?),
            decoder: Some(decoder?),
            joiner: Some(joiner?),
        };
        config.model_config.tokens = Some(tokens?);
        config.model_config.model_type = Some("nemo_transducer".to_string());
        config.model_config.num_threads = self.threads;
        config.model_config.provider = Some("cpu".to_string());
        let recognizer = sherpa_onnx::OfflineRecognizer::create(&config).with_context(|| {
            crate::tr!(
                en = "sherpa-onnx cannot load Parakeet from {}",
                fr = "sherpa-onnx ne charge pas Parakeet depuis {}",
                model_dir.display()
            )
        })?;
        *self.recognizer.lock().unwrap_or_else(|e| e.into_inner()) = Some(recognizer);
        Ok(())
    }

    /// Une demi-seconde de silence : le premier vrai énoncé ne paie pas la
    /// mise en route d'ONNX Runtime.
    fn warm_up(&self) -> Result<()> {
        self.transcribe(&[0.0; VAD_SAMPLE_RATE as usize / 2], "", &Cancel::never())
            .map(|_| ())
    }

    /// L'abandon est vérifié avant et après le calcul : sherpa-onnx ne
    /// l'interrompt pas en route.
    fn transcribe(&self, samples: &[f32], _language: &str, cancel: &Cancel) -> Result<String> {
        cancel.check()?;
        let guard = self.recognizer.lock().unwrap_or_else(|e| e.into_inner());
        cancel.check()?;
        let recognizer = guard.as_ref().context(crate::tr!(
            "Parakeet model not loaded",
            "modèle Parakeet non chargé"
        ))?;
        let stream = recognizer.create_stream();
        stream.accept_waveform(VAD_SAMPLE_RATE as i32, samples);
        recognizer.decode(&stream);
        cancel.check()?;
        let result = stream.get_result().context(crate::tr!(
            "Parakeet gives no result",
            "Parakeet ne rend aucun résultat"
        ))?;
        Ok(result.text.trim().to_string())
    }

    fn release(&self) {
        *self.recognizer.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn same() -> Result<String> {
        Ok("1.31.0".to_string())
    }

    fn two_copies() -> Result<String> {
        crate::stt::sherpa::check_same_runtime("1.31.0", "1.28.2").map(|()| "1.31.0".to_string())
    }

    /// Un dossier de modèle aux fichiers vides, sauf `missing`.
    fn model_without(name: &str, missing: Option<&str>) -> PathBuf {
        let dir = crate::paths::test_scratch(&format!("sherpa-parakeet-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in FILES.iter().filter(|f| Some(**f) != missing) {
            std::fs::write(dir.join(file), b"").unwrap();
        }
        dir
    }

    #[test]
    fn a_missing_model_file_is_named() {
        for missing in FILES {
            let dir = model_without("fichier-absent", Some(missing));
            let err = ParakeetSherpa::with_runtime(4, same)
                .load(&dir)
                .unwrap_err()
                .to_string();
            assert!(err.contains(missing), "{missing} : {err}");
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    #[test]
    fn two_different_runtimes_are_refused_before_loading() {
        // Le dossier n'existe pas : seule la vérification d'ORT peut parler.
        let dir = crate::paths::test_scratch("sherpa-parakeet-jamais-cree");
        let err = ParakeetSherpa::with_runtime(4, two_copies)
            .load(&dir)
            .unwrap_err()
            .to_string();
        assert!(err.contains("1.31.0") && err.contains("1.28.2"), "{err}");
        assert!(
            !err.contains("encoder"),
            "refusé avant de chercher le modèle : {err}"
        );
    }

    #[test]
    fn at_most_four_threads_and_never_zero() {
        assert_eq!(ParakeetSherpa::new(8).threads, 4);
        assert_eq!(ParakeetSherpa::new(4).threads, 4);
        assert_eq!(ParakeetSherpa::new(2).threads, 2);
        assert_eq!(ParakeetSherpa::new(0).threads, 1);
    }

    #[test]
    fn transcribing_before_loading_is_an_error() {
        let err = ParakeetSherpa::with_runtime(1, same)
            .transcribe(&[0.0; 160], "fr", &Cancel::never())
            .unwrap_err();
        assert!(err.to_string().contains("Parakeet"), "{err}");
    }

    /// Le texte que le banc du spike (`bench`, sherpa-onnx 2.0.2, 4 fils,
    /// lecture d'un bloc) rend pour `test_wavs/fr.wav` du dépôt du paquet. Le
    /// banc passe l'audio pour du 16 kHz sans le rééchantillonner ; ramené à
    /// 16 kHz, l'audio donne le même texte.
    const FRENCH_SAMPLE: &str =
        "Ne vous demandez pas ce que votre pays peut faire pour vous. Demandez-vous plutôt ce que vous pouvez faire pour lui.";

    /// Le vrai modèle : `DYSFLOW_MODELS/parakeet_tdt_v3_int8/`, avec le
    /// `test_wavs/fr.wav` du dépôt. Sans la variable ou sans le modèle,
    /// l'essai échoue (jamais à vide).
    ///
    ///   DYSFLOW_MODELS=… target/ort-gate/dysflow_lib_tests stt::sherpa -- --ignored
    #[test]
    #[ignore = "demande le modèle (DYSFLOW_MODELS) et la bibliothèque de sherpa-onnx (scripts/ort-gate.sh)"]
    fn parakeet_reads_the_french_sample() {
        let models =
            std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS : le dossier des modèles");
        let dir = PathBuf::from(models).join("parakeet_tdt_v3_int8");
        let wav = dir.join("test_wavs").join("fr.wav");
        assert!(
            wav.is_file(),
            "pas de test_wavs/fr.wav dans {}",
            dir.display()
        );
        let wave = sherpa_onnx::Wave::read(wav.to_str().unwrap()).expect("fr.wav illisible");
        // Le fichier du dépôt est à 22,05 kHz : ramené aux 16 kHz que
        // `Recognizer` demande, comme sherpa-onnx le ferait lui-même.
        assert_eq!(wave.sample_rate(), 22_050);
        let resampler =
            sherpa_onnx::LinearResampler::create(wave.sample_rate(), VAD_SAMPLE_RATE as i32)
                .unwrap();
        let samples = resampler.resample(wave.samples(), true);
        let parakeet = ParakeetSherpa::new(4);
        parakeet.load(&dir).unwrap();
        parakeet.warm_up().unwrap();
        let heard = parakeet
            .transcribe(&samples, "fr", &Cancel::never())
            .unwrap();
        println!("Parakeet int8 ({}) : {heard}", dir.display());
        assert_eq!(heard, FRENCH_SAMPLE);
        parakeet.release();
    }
}

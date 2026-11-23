//! Silero VAD par sherpa-onnx : le détecteur de parole de la dictée sous
//! Windows et Linux.

use crate::stt::detector::{SpeechDetector, FRAME};
use crate::stt::sherpa::{model_file, same_runtime, VAD_SAMPLE_RATE};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Seuil de Silero : une trame est de la parole au-delà de cette probabilité.
const THRESHOLD: f32 = 0.5;
/// Parole et silence minimaux de Silero, en secondes. Courts : la découpe
/// (`Segmenter`) fait le reste (silence de 13 trames, parole de 0,25 s) ;
/// une parole de plus de 0,1 s est entendue avant la fin de la marge de
/// 0,2 s que la découpe garde devant.
const MIN_SPEECH: f32 = 0.1;
const MIN_SILENCE: f32 = 0.1;
/// Au-delà, Silero coupe de lui-même ; la découpe coupe avant, à 12 s.
const MAX_SPEECH: f32 = 20.0;
/// Le tampon des segments de Silero, en secondes : chaque trame les vide.
const BUFFER_SECONDS: f32 = MAX_SPEECH + 10.0;

/// Silero VAD, trame par trame (`FRAME`, sa fenêtre à 16 kHz) : une trame est
/// de la parole quand Silero dit en entendre (`detected`).
pub struct SileroDetector {
    vad: sherpa_onnx::VoiceActivityDetector,
}

impl SileroDetector {
    /// Charge `model` (`silero_vad.onnx`), après la vérification d'ONNX
    /// Runtime (`same_runtime`).
    pub fn new(model: &Path) -> Result<Self> {
        Self::with_runtime(model, same_runtime)
    }

    /// `new`, la vérification d'ONNX Runtime passée en argument : les essais
    /// ne touchent pas à celle, gardée, du processus.
    fn with_runtime(model: &Path, runtime: fn() -> Result<String>) -> Result<Self> {
        // Avant que sherpa-onnx ne crée son environnement ONNX Runtime, même
        // quand `runtime` n'est pas `same_runtime` (les essais).
        crate::tts::kokoro::onnx::disable_telemetry();
        runtime()?;
        let model = model_file(model)?;
        let config = sherpa_onnx::VadModelConfig {
            silero_vad: sherpa_onnx::SileroVadModelConfig {
                model: Some(model),
                threshold: THRESHOLD,
                min_silence_duration: MIN_SILENCE,
                min_speech_duration: MIN_SPEECH,
                window_size: FRAME as i32,
                max_speech_duration: MAX_SPEECH,
            },
            sample_rate: VAD_SAMPLE_RATE as i32,
            num_threads: 1,
            provider: Some("cpu".to_string()),
            ..Default::default()
        };
        let vad = sherpa_onnx::VoiceActivityDetector::create(&config, BUFFER_SECONDS).context(
            crate::tr!(
                "sherpa-onnx cannot create Silero VAD",
                "sherpa-onnx ne crée pas Silero VAD"
            ),
        )?;
        Ok(Self { vad })
    }
}

impl SpeechDetector for SileroDetector {
    fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
        if frame.len() != FRAME {
            bail!(
                "{}",
                crate::tr!(
                    en = "a detector frame is {FRAME} samples, got {}",
                    fr = "une trame du détecteur fait {FRAME} échantillons, reçu {}",
                    frame.len()
                )
            );
        }
        self.vad.accept_waveform(frame);
        // Les segments que Silero garde ne servent pas : la découpe garde
        // l'audio elle-même.
        self.vad.clear();
        Ok(self.vad.detected())
    }

    fn reset(&mut self) {
        self.vad.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same() -> Result<String> {
        Ok("1.31.0".to_string())
    }

    #[test]
    fn a_missing_silero_model_is_named() {
        let model = crate::paths::test_scratch("sherpa-silero-absent").join("silero_vad.onnx");
        let err = SileroDetector::with_runtime(&model, same)
            .err()
            .expect("modèle absent")
            .to_string();
        assert!(err.contains("silero_vad.onnx"), "{err}");
    }

    #[test]
    fn two_different_runtimes_refuse_silero_before_loading() {
        let two =
            || crate::stt::sherpa::check_same_runtime("1.31.0", "1.28.2").map(|()| String::new());
        let model = crate::paths::test_scratch("sherpa-silero-jamais-cree").join("silero_vad.onnx");
        let err = SileroDetector::with_runtime(&model, two)
            .err()
            .expect("refusé")
            .to_string();
        assert!(
            err.contains("1.28.2") && !err.contains("silero_vad.onnx"),
            "{err}"
        );
    }

    /// Une trame d'une autre longueur que `FRAME`, vide comprise, est une
    /// erreur rendue avant d'atteindre sherpa-onnx ; une trame juste passe.
    /// Lancé par `scripts/ort-gate.sh`, avec le Silero de la porte :
    ///
    ///   DYSFLOW_SILERO_VAD=… target/ort-gate/dysflow_lib_tests stt::sherpa::vad --ignored
    #[test]
    #[ignore = "demande silero_vad.onnx (DYSFLOW_SILERO_VAD) et la bibliothèque de sherpa-onnx (scripts/ort-gate.sh)"]
    fn a_wrong_frame_is_an_error_before_silero() {
        let model =
            std::env::var_os("DYSFLOW_SILERO_VAD").expect("DYSFLOW_SILERO_VAD : le modèle Silero");
        let mut detector = SileroDetector::new(Path::new(&model)).unwrap();
        for len in [0, 1, FRAME - 1, FRAME + 1, 2 * FRAME] {
            let err = detector.is_speech(&vec![0.0; len]).unwrap_err().to_string();
            assert!(err.contains(&len.to_string()), "{len} : {err}");
        }
        assert!(!detector.is_speech(&[0.0; FRAME]).unwrap(), "du silence");
    }

    /// Un faux `silero_vad.onnx`, de la bonne taille (`stt::SILERO_BYTES`)
    /// mais d'un autre contenu, donné à sherpa-onnx : ce que fait
    /// `SileroDetector::new`, mesuré dans un processus fils (un abandon
    /// d'ONNX Runtime ou de protobuf y est un signal, pas la perte du
    /// lanceur d'essais). Le fils écrit `ISSUE : …` ; le père l'affiche avec
    /// le statut de sortie. Lancé par `scripts/ort-gate.sh`.
    ///
    /// Mesuré (sherpa-onnx 2.0.2, ORT 1.31.0, macOS) : une erreur lisible,
    /// sans abandon : « Protobuf parsing failed » sur la sortie d'erreur de
    /// sherpa-onnx, puis `create` rend une erreur (« sherpa-onnx ne crée pas
    /// Silero VAD »), statut 0. Le paquet ne va plus jusque-là : son Silero
    /// est vérifié par empreinte avant (`stt::sherpa_detector`).
    #[test]
    #[ignore = "demande la bibliothèque de sherpa-onnx et ONNX Runtime (scripts/ort-gate.sh)"]
    fn a_garbage_silero_model_of_the_right_size_is_measured_in_a_child() {
        const CHILD: &str = "DYSFLOW_SILERO_GARBAGE_CHILD";
        if let Some(model) = std::env::var_os(CHILD) {
            match SileroDetector::new(Path::new(&model)) {
                Ok(_) => println!("ISSUE : chargé sans erreur"),
                Err(err) => println!("ISSUE : erreur : {err:#}"),
            }
            return;
        }
        let model = crate::paths::test_scratch("silero-faux").join("silero_vad.onnx");
        let mut state: u32 = 0x2545_f491;
        let garbage: Vec<u8> = (0..crate::stt::SILERO_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        std::fs::write(&model, garbage).unwrap();
        let name = format!("{}::a_garbage_silero_model_of_the_right_size_is_measured_in_a_child", module_path!());
        let name = name.strip_prefix("dysflow::").unwrap_or(&name).to_string();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name.as_str(), "--exact", "--ignored", "--nocapture", "--test-threads=1"])
            .env(CHILD, &model)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // La ligne du fils suit « test … ... » sur la même ligne.
        let issue = stdout.lines().find_map(|line| line.split_once("ISSUE : ").map(|(_, issue)| issue));
        println!("Silero faux, {} octets : statut {} ; {issue:?}", crate::stt::SILERO_BYTES, out.status);
        println!("sortie d'erreur du fils :\n{stderr}");
        assert!(
            out.status.success() && issue.is_some_and(|line| line.contains("erreur")),
            "le faux modèle n'est pas refusé proprement : statut {}, {issue:?}\n{stdout}\n{stderr}",
            out.status
        );
    }

    /// Silero chargé et lancé par sherpa-onnx, qui crée son propre
    /// environnement ONNX Runtime, ne laisse aucune trace de la télémétrie
    /// d'ORT (`onnx::assert_no_telemetry_trace`). Mêmes variables que
    /// l'essai précédent.
    #[test]
    #[ignore = "demande silero_vad.onnx (DYSFLOW_SILERO_VAD) et la bibliothèque de sherpa-onnx (scripts/ort-gate.sh)"]
    fn silero_leaves_no_telemetry_trace() {
        let model =
            std::env::var_os("DYSFLOW_SILERO_VAD").expect("DYSFLOW_SILERO_VAD : le modèle Silero");
        crate::tts::kokoro::onnx::assert_no_telemetry_trace(
            &format!("{}::silero_leaves_no_telemetry_trace", module_path!()),
            || {
                let mut detector = SileroDetector::new(Path::new(&model)).unwrap();
                assert!(!detector.is_speech(&[0.0; FRAME]).unwrap(), "du silence");
            },
        );
    }
}

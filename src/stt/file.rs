//! Dicter un fichier WAV (`dysflow --transcribe`) : le fichier passe par
//! `Dictation`, comme le micro le ferait, par morceaux de 100 ms donnés aussi
//! vite que possible (le temps n'est pas simulé). Le texte final part sur la
//! sortie standard, seul ; la progression, sur la sortie d'erreur.

use crate::stt::dictation::{Dictation, Engine, Event};
use anyhow::Result;
use std::path::Path;

/// Fréquence demandée : celle des reconnaisseurs (`Recognizer`).
pub const SAMPLE_RATE: u32 = 16_000;
/// Un morceau d'audio donné à `Dictation::push` : 100 ms.
pub const CHUNK: usize = SAMPLE_RATE as usize / 10;

/// Le chemin entre guillemets pour le shell de l'utilisateur : apostrophes
/// sous Unix (une apostrophe du chemin est protégée), guillemets sous Windows.
fn quoted(path: &Path) -> String {
    let text = path.display().to_string();
    if cfg!(windows) {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// La commande `ffmpeg` qui convertit `path` en WAV 16 kHz mono, à côté de
/// lui (`<nom>-16k.wav`).
pub fn ffmpeg_command(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "audio".to_string());
    let converted = path.with_file_name(format!("{stem}-16k.wav"));
    format!(
        "ffmpeg -i {} -ar 16000 -ac 1 {}",
        quoted(path),
        quoted(&converted)
    )
}

/// Le message d'un fichier qui n'est pas un WAV 16 kHz mono : ce qu'il est,
/// et la commande qui le convertit.
fn unsupported(path: &Path, found: &str) -> anyhow::Error {
    let command = ffmpeg_command(path);
    anyhow::anyhow!(crate::tr!(
        en = "{}: not a 16 kHz mono WAV (16-bit PCM or 32-bit float): {found}. Convert it with:\n  {command}",
        fr = "{} : pas un WAV mono à 16 kHz (PCM 16 bits ou flottant 32 bits) : {found}. Convertissez-le avec :\n  {command}",
        path.display()
    ))
}

/// Un WAV 16 kHz mono, PCM 16 bits ou flottant 32 bits, lu au fil de
/// l'eau : jamais le fichier entier en mémoire. Tout autre fichier est refusé
/// à l'ouverture, avec la commande `ffmpeg` qui le convertit.
pub struct Wav {
    path: std::path::PathBuf,
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    float: bool,
}

impl Wav {
    pub fn open(path: &Path) -> Result<Self> {
        anyhow::ensure!(
            path.is_file(),
            crate::tr!(
                en = "file not found: {}",
                fr = "fichier introuvable : {}",
                path.display()
            )
        );
        let reader = match hound::WavReader::open(path) {
            Ok(reader) => reader,
            Err(err) => {
                let found = crate::tr!(en = "unreadable ({err})", fr = "illisible ({err})");
                return Err(unsupported(path, &found));
            }
        };
        let spec = reader.spec();
        let supported = spec.channels == 1
            && spec.sample_rate == SAMPLE_RATE
            && matches!(
                (spec.sample_format, spec.bits_per_sample),
                (hound::SampleFormat::Int, 16) | (hound::SampleFormat::Float, 32)
            );
        if !supported {
            let kind = match spec.sample_format {
                hound::SampleFormat::Int => "PCM",
                hound::SampleFormat::Float => "float",
            };
            let found = crate::tr!(
                en = "it is {} Hz, {} channel(s), {}-bit {kind}",
                fr = "il est à {} Hz, {} canal(aux), {} bits {kind}",
                spec.sample_rate,
                spec.channels,
                spec.bits_per_sample
            );
            return Err(unsupported(path, &found));
        }
        Ok(Self {
            path: path.to_path_buf(),
            float: spec.sample_format == hound::SampleFormat::Float,
            reader,
        })
    }

    /// Les morceaux de 100 ms (`CHUNK`), le dernier plus court. Un fichier
    /// tronqué ou abîmé donne une erreur à l'endroit où il l'est.
    pub fn chunks(mut self) -> impl Iterator<Item = Result<Vec<f32>>> {
        std::iter::from_fn(move || {
            let mut chunk = Vec::with_capacity(CHUNK);
            while chunk.len() < CHUNK {
                let sample = if self.float {
                    self.reader.samples::<f32>().next()
                } else {
                    self.reader
                        .samples::<i16>()
                        .next()
                        .map(|s| s.map(|s| f32::from(s) / 32_768.0))
                };
                match sample {
                    Some(Ok(sample)) => chunk.push(sample),
                    Some(Err(err)) => {
                        return Some(Err(anyhow::Error::new(err).context(crate::tr!(
                            en = "{} is cut short or damaged",
                            fr = "{} est tronqué ou abîmé",
                            self.path.display()
                        ))));
                    }
                    None => break,
                }
            }
            (!chunk.is_empty()).then_some(Ok(chunk))
        })
    }
}

/// Tout le fichier d'un coup, pour les essais : `Wav::chunks` est la lecture
/// du lecteur.
#[cfg(test)]
pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut samples = Vec::new();
    for chunk in Wav::open(path)?.chunks() {
        samples.extend(chunk?);
    }
    Ok(samples)
}

/// Ce qu'un événement de la dictée dit sur la sortie d'erreur, s'il en dit.
fn progress_line(event: &Event) -> Option<String> {
    match event {
        Event::Loading => {
            Some(crate::tr!("Loading the model…", "Chargement du modèle…").to_string())
        }
        Event::Loaded(took) => Some(crate::tr!(
            en = "Model loaded in {:.1} s",
            fr = "Modèle chargé en {:.1} s",
            took.as_secs_f64()
        )),
        Event::Committed { text, .. } if !text.trim().is_empty() => {
            Some(format!("> {}", text.trim()))
        }
        Event::Failed(message) => Some(crate::tr!(
            en = "Failed: {message}",
            fr = "Échec : {message}"
        )),
        _ => None,
    }
}

/// Le plus d'audio qui attend le modèle quand un fichier le nourrit : 5 s
/// (`Dictation::push_bounded`). Un fichier peut attendre ; la mémoire d'une
/// dictée reste donc de cet ordre, plus deux énoncés de 12 s au plus, que le
/// fichier ait une minute ou dix heures (R47).
pub const MAX_BACKLOG: usize = 5 * SAMPLE_RATE as usize;

/// Donne `chunks` à `dictation` sans jamais laisser plus de `limit`
/// échantillons attendre le modèle ; `observe` reçoit l'attente après chaque
/// morceau. La première erreur de lecture arrête tout ; une dictée arrêtée
/// (`push_bounded` rend `false`) arrête la lecture du fichier : `finish` dit
/// pourquoi.
fn feed(
    dictation: &Dictation,
    chunks: impl IntoIterator<Item = Result<Vec<f32>>>,
    limit: usize,
    mut observe: impl FnMut(usize),
) -> Result<()> {
    for chunk in chunks {
        if !dictation.push_bounded(&chunk?, limit) {
            break;
        }
        observe(dictation.backlog());
    }
    Ok(())
}

/// Dicte `chunks` (16 kHz, mono) par `engine` : l'audio est donné par
/// morceaux, au rythme du modèle (`MAX_BACKLOG`), puis la fin est demandée.
/// `progress` reçoit chaque ligne de progression, sur le fil de la dictée.
/// Rend le texte entier. Une erreur de lecture annule la dictée.
pub fn dictate(
    chunks: impl IntoIterator<Item = Result<Vec<f32>>>,
    engine: Engine,
    model_dir: &Path,
    language: &str,
    progress: impl Fn(String) + Send + 'static,
) -> Result<String> {
    let dictation = Dictation::start(engine, model_dir, language, move |event| {
        if let Some(line) = progress_line(&event) {
            progress(line);
        }
    });
    feed(&dictation, chunks, MAX_BACKLOG, |_| {})?;
    dictation.finish()
}

/// Le texte seul sur `out`. Rien entendu : aucun texte, et `progress` le dit.
pub fn print_text(
    text: &str,
    out: &mut dyn std::io::Write,
    progress: impl Fn(String),
) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        progress(crate::tr!("No speech heard.", "Aucune parole entendue.").to_string());
        return Ok(());
    }
    writeln!(out, "{text}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::detector::SpeechDetector;
    use crate::stt::recognizer::Recognizer;
    use crate::tts::Cancel;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn scratch(name: &str) -> PathBuf {
        crate::paths::test_scratch(&format!("stt-file-{name}"))
    }

    fn write_wav(path: &Path, rate: u32, channels: u16, bits: u16, float: bool, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: if float {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for s in samples {
            match (float, bits) {
                (true, _) => writer.write_sample(*s).unwrap(),
                (false, 16) => writer.write_sample((s * 32_767.0) as i16).unwrap(),
                (false, _) => writer.write_sample((s * 8_388_607.0) as i32).unwrap(),
            }
        }
        writer.finalize().unwrap();
    }

    /// Trois énoncés d'amplitudes 0,3, 0,5 et 0,7, séparés de 0,8 s de silence.
    fn three_utterances() -> Vec<f32> {
        let mut audio = Vec::new();
        for amplitude in [0.3_f32, 0.5, 0.7] {
            audio.resize(audio.len() + 12_800, 0.0);
            audio.resize(audio.len() + 8_000, amplitude);
        }
        audio.resize(audio.len() + 12_800, 0.0);
        audio
    }

    /// Un détecteur d'amplitude : de la parole au-delà de 0,05 de RMS.
    struct Loud;
    impl SpeechDetector for Loud {
        fn is_speech(&mut self, frame: &[f32]) -> Result<bool> {
            let rms = (frame.iter().map(|x| x * x).sum::<f32>() / frame.len() as f32).sqrt();
            Ok(rms > 0.05)
        }
        fn reset(&mut self) {}
    }

    /// Un reconnaisseur simulé : le texte vient de l'amplitude de l'audio
    /// (0,3 « bonjour », 0,5 « tout », 0,7 « le monde »), pas de l'ordre des
    /// appels ; il note la longueur de chaque lecture.
    #[derive(Default)]
    struct Fake {
        reads: Mutex<Vec<usize>>,
        fail_on_half: bool,
        fail_to_load: bool,
    }

    impl Recognizer for Fake {
        fn load(&self, _: &Path) -> Result<()> {
            anyhow::ensure!(!self.fail_to_load, "fichier du modèle absent");
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn transcribe(&self, samples: &[f32], _: &str, _: &Cancel) -> Result<String> {
            self.reads.lock().unwrap().push(samples.len());
            let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            Ok(match peak {
                p if (0.25..0.4).contains(&p) => "bonjour",
                p if (0.4..0.6).contains(&p) => {
                    anyhow::ensure!(!self.fail_on_half, "lecture impossible");
                    "tout"
                }
                p if p >= 0.6 => "le monde",
                _ => "",
            }
            .to_string())
        }
        fn release(&self) {}
    }

    fn chunks_of(audio: &[f32]) -> Vec<Result<Vec<f32>>> {
        audio.chunks(CHUNK).map(|c| Ok(c.to_vec())).collect()
    }

    fn run(fake: Arc<Fake>, audio: &[f32]) -> (Result<String>, Vec<String>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let text = dictate(
            chunks_of(audio),
            Engine::Segmented(fake, Box::new(Loud)),
            Path::new("modele"),
            "fr",
            move |line| sink.lock().unwrap().push(line),
        );
        let lines = lines.lock().unwrap().clone();
        (text, lines)
    }

    #[test]
    fn a_pcm16_mono_16k_file_is_read_as_floats() {
        let path = scratch("pcm16").join("a.wav");
        write_wav(&path, 16_000, 1, 16, false, &[0.0, 0.5, -0.5]);
        let samples = read_wav(&path).unwrap();
        assert_eq!(samples.len(), 3);
        assert!(
            (samples[1] - 0.5).abs() < 1e-3 && (samples[2] + 0.5).abs() < 1e-3,
            "{samples:?}"
        );
    }

    #[test]
    fn a_float32_mono_16k_file_is_read_as_is() {
        let path = scratch("f32").join("a.wav");
        write_wav(&path, 16_000, 1, 32, true, &[0.25, -0.75]);
        assert_eq!(read_wav(&path).unwrap(), vec![0.25, -0.75]);
    }

    #[test]
    fn anything_else_is_refused_with_the_ffmpeg_command() {
        let dir = scratch("refused");
        let cases: [(&str, u32, u16, u16, bool); 4] = [
            ("rate", 44_100, 1, 16, false),
            ("stereo", 16_000, 2, 16, false),
            ("deep", 16_000, 1, 24, false),
            ("float16k-stereo", 16_000, 2, 32, true),
        ];
        for (name, rate, channels, bits, float) in cases {
            let path = dir.join(format!("{name}.wav"));
            write_wav(&path, rate, channels, bits, float, &[0.1, 0.2, 0.3, 0.4]);
            let err = read_wav(&path).unwrap_err().to_string();
            assert!(err.contains(&ffmpeg_command(&path)), "{name} : {err}");
            assert!(err.contains("-ar 16000 -ac 1"), "{name} : {err}");
        }
    }

    #[test]
    fn the_message_follows_the_interface_language() {
        let path = scratch("langue").join("a.wav");
        write_wav(&path, 8_000, 1, 16, false, &[0.1, 0.2]);
        let en = crate::i18n::with_ui_lang(crate::i18n::UiLang::En, || {
            read_wav(&path).unwrap_err().to_string()
        });
        let fr = crate::i18n::with_ui_lang(crate::i18n::UiLang::Fr, || {
            read_wav(&path).unwrap_err().to_string()
        });
        assert!(
            en.contains("not a 16 kHz mono WAV") && en.contains("8000 Hz"),
            "{en}"
        );
        assert!(
            fr.contains("pas un WAV mono à 16 kHz") && fr.contains("8000 Hz"),
            "{fr}"
        );
    }

    #[test]
    fn a_file_that_is_no_wav_is_refused_the_same_way() {
        let path = scratch("texte").join("notes.wav");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "ceci n'est pas un WAV").unwrap();
        let err = read_wav(&path).unwrap_err().to_string();
        assert!(err.contains(&ffmpeg_command(&path)), "{err}");
    }

    #[test]
    fn a_missing_file_is_named() {
        let path = scratch("absent").join("rien.wav");
        let err = read_wav(&path).unwrap_err().to_string();
        assert!(err.contains("rien.wav"), "{err}");
        assert!(!err.contains("ffmpeg"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn the_command_quotes_the_path_of_the_user() {
        let path = Path::new("/home/Élodie Martin/l'été 2026.m4a");
        assert_eq!(
            ffmpeg_command(path),
            r"ffmpeg -i '/home/Élodie Martin/l'\''été 2026.m4a' -ar 16000 -ac 1 '/home/Élodie Martin/l'\''été 2026-16k.wav'"
        );
    }

    /// « Élodie Martin » composé (NFC), puis décomposé (NFD) : deux dossiers
    /// à part, car sous APFS le second retrouverait le premier.
    #[test]
    fn accented_paths_are_read_in_nfc_and_nfd() {
        let nfc = "\u{c9}lodie Martin";
        let nfd = "E\u{301}lodie Martin";
        assert_ne!(nfc, nfd);
        for (kind, name) in [("nfc", nfc), ("nfd", nfd)] {
            let path = scratch(&format!("accents-{kind}"))
                .join(name)
                .join("dictée.wav");
            write_wav(&path, 16_000, 1, 16, false, &[0.1; 100]);
            assert_eq!(read_wav(&path).unwrap().len(), 100, "{kind}");
            let (text, _) = run(Arc::new(Fake::default()), &read_wav(&path).unwrap());
            assert!(text.is_ok(), "{kind}");
        }
    }

    #[test]
    fn the_file_goes_through_dictation_in_chunks_of_100_ms() {
        let audio = three_utterances();
        let fake = Arc::new(Fake::default());
        let (text, lines) = run(fake.clone(), &audio);
        assert_eq!(text.unwrap(), "bonjour tout le monde");
        // Chaque énoncé est lu à part : jamais le fichier d'un bloc.
        let reads = fake.reads.lock().unwrap();
        assert!(reads.len() >= 3, "{reads:?}");
        assert!(
            reads.iter().all(|n| *n < audio.len() / 2),
            "{reads:?} sur {}",
            audio.len()
        );
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.starts_with("> "))
                .cloned()
                .collect::<Vec<_>>(),
            ["> bonjour", "> tout", "> le monde"]
        );
        assert!(
            lines
                .first()
                .is_some_and(|l| l.contains("Loading") || l.contains("Chargement")),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("loaded") || l.contains("chargé")),
            "{lines:?}"
        );
    }

    #[test]
    fn stdout_gets_the_text_alone_and_the_progress_stays_apart() {
        let (text, lines) = run(Arc::new(Fake::default()), &three_utterances());
        let mut out = Vec::new();
        print_text(&text.unwrap(), &mut out, |_| {
            panic!("du texte : rien à dire")
        })
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "bonjour tout le monde\n");
        assert!(lines.len() >= 5, "{lines:?}");
    }

    #[test]
    fn a_file_without_speech_prints_nothing_and_says_so() {
        let (text, _) = run(Arc::new(Fake::default()), &vec![0.0; 32_000]);
        let said = Mutex::new(Vec::new());
        let mut out = Vec::new();
        print_text(&text.unwrap(), &mut out, |l| said.lock().unwrap().push(l)).unwrap();
        assert!(out.is_empty());
        assert_eq!(said.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_unreadable_utterance_is_reported_and_the_others_kept() {
        let fake = Arc::new(Fake {
            fail_on_half: true,
            ..Fake::default()
        });
        let (text, lines) = run(fake, &three_utterances());
        assert_eq!(text.unwrap(), "bonjour le monde");
        assert!(
            lines.iter().any(|l| l.contains("lecture impossible")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_model_that_does_not_load_ends_the_dictation_with_an_error() {
        let fake = Arc::new(Fake {
            fail_to_load: true,
            ..Fake::default()
        });
        let (text, lines) = run(fake, &three_utterances());
        let err = text.unwrap_err().to_string();
        assert!(err.contains("fichier du modèle absent"), "{err}");
        assert!(
            lines.iter().any(|l| l.contains("fichier du modèle absent")),
            "{lines:?}"
        );
    }

    /// Beaucoup d'énoncés, pour un reconnaisseur lent : `n` énoncés de 0,5 s
    /// séparés de 0,8 s de silence.
    fn many_utterances(n: usize) -> Vec<f32> {
        let mut audio = Vec::new();
        for _ in 0..n {
            audio.resize(audio.len() + 12_800, 0.0);
            audio.resize(audio.len() + 8_000, 0.3);
        }
        audio.resize(audio.len() + 12_800, 0.0);
        audio
    }

    /// Le plus d'audio en attente vu pendant que `audio` est donné par `feed`
    /// avec `limit`, à un reconnaisseur qui met `read_ms` par lecture.
    fn worst_backlog(audio: &[f32], limit: usize, read_ms: u64) -> usize {
        struct Slow(Fake, u64);
        impl Recognizer for Slow {
            fn load(&self, dir: &Path) -> Result<()> {
                self.0.load(dir)
            }
            fn warm_up(&self) -> Result<()> {
                Ok(())
            }
            fn transcribe(
                &self,
                samples: &[f32],
                language: &str,
                cancel: &Cancel,
            ) -> Result<String> {
                std::thread::sleep(std::time::Duration::from_millis(self.1));
                self.0.transcribe(samples, language, cancel)
            }
            fn release(&self) {}
        }
        let engine = Engine::Segmented(Arc::new(Slow(Fake::default(), read_ms)), Box::new(Loud));
        let dictation = Dictation::start(engine, Path::new("modele"), "fr", |_| {});
        let mut worst = 0;
        feed(&dictation, chunks_of(audio), limit, |b| {
            worst = worst.max(b)
        })
        .unwrap();
        let text = dictation.finish().unwrap();
        assert!(text.starts_with("bonjour bonjour"), "{text}");
        worst
    }

    /// R47 : la mémoire d'une dictée de fichier ne dépend pas de sa longueur.
    /// Cent vingt énoncés (près de 3 minutes), un reconnaisseur lent : l'attente
    /// reste sous la limite, plus un morceau et les énoncés que le fil vient de
    /// clore. Sans la limite (`usize::MAX`, comme `push`), elle monte avec le
    /// fichier : l'essai le voit.
    #[test]
    fn the_backlog_of_a_file_stays_bounded_whatever_its_length() {
        let audio = many_utterances(120);
        let bound = MAX_BACKLOG + CHUNK + 2 * crate::stt::segmenter::MAX_SEGMENT;
        let bounded = worst_backlog(&audio, MAX_BACKLOG, 2);
        assert!(bounded <= bound, "{bounded} > {bound}");
        let unbounded = worst_backlog(&audio, usize::MAX, 2);
        assert!(
            unbounded > 2 * bound,
            "sans limite : {unbounded}, avec : {bounded}"
        );
    }

    /// Un modèle qui ne se charge pas arrête le fil : l'audio qui attend ne
    /// serait jamais pris, la source ne doit pas attendre pour toujours.
    #[test]
    fn a_dead_worker_does_not_hold_the_file_back() {
        let fake = Arc::new(Fake {
            fail_to_load: true,
            ..Fake::default()
        });
        let (text, _) = run(fake, &many_utterances(20));
        assert!(text.is_err());
    }

    /// Un modèle qui ne se charge pas : la lecture du fichier s'arrête tout de
    /// suite (quelques morceaux lus, pas le fichier), et l'erreur du chargement
    /// est rendue. Un fichier de 4 Go ne finit pas en mémoire. On compte les
    /// morceaux lus, pas le temps.
    #[test]
    fn a_dead_worker_stops_the_reading_of_the_file() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let read = Arc::new(AtomicUsize::new(0));
        let counter = read.clone();
        let source = (0..100_000).map(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(vec![0.1; CHUNK])
        });
        let fake = Arc::new(Fake {
            fail_to_load: true,
            ..Fake::default()
        });
        let err = dictate(
            source,
            Engine::Segmented(fake, Box::new(Loud)),
            Path::new("modele"),
            "fr",
            |_| {},
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("fichier du modèle absent"), "{err}");
        let read = read.load(Ordering::SeqCst);
        // Au plus de quoi remplir l'attente (50 morceaux) avant que le fil
        // mort soit vu, jamais les 100 000.
        assert!(read <= MAX_BACKLOG / CHUNK + 10, "{read} morceaux lus");
    }

    #[test]
    fn a_truncated_file_is_an_error_not_a_short_dictation() {
        let path = scratch("tronque").join("a.wav");
        write_wav(&path, 16_000, 1, 16, false, &[0.1; 8_000]);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 4_000]).unwrap();
        let failed = match Wav::open(&path) {
            Err(_) => true,
            Ok(wav) => wav.chunks().any(|c| c.is_err()),
        };
        assert!(failed);
    }

    /// Le lecteur rend des morceaux de 100 ms, le dernier plus court.
    #[test]
    fn the_reader_streams_chunks_of_100_ms() {
        let path = scratch("morceaux").join("a.wav");
        write_wav(&path, 16_000, 1, 16, false, &vec![0.1; CHUNK * 2 + 10]);
        let sizes: Vec<usize> = Wav::open(&path)
            .unwrap()
            .chunks()
            .map(|c| c.unwrap().len())
            .collect();
        assert_eq!(sizes, [CHUNK, CHUNK, 10]);
    }

    /// Le vrai Parakeet, par `engine_for`, sur `test_wavs/fr.wav` du paquet,
    /// ramené à 16 kHz par le rééchantillonneur de sherpa-onnx, écrit en
    /// WAV puis relu par `read_wav`. Lancé par `scripts/ort-gate.sh` quand
    /// `DYSFLOW_MODELS` est posé (comme les essais de `stt::sherpa::parakeet`).
    #[cfg(feature = "stt-sherpa")]
    #[test]
    #[ignore = "demande le modèle (DYSFLOW_MODELS) et la bibliothèque de sherpa-onnx (scripts/ort-gate.sh)"]
    fn the_real_model_dictates_the_french_sample() {
        let models =
            std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS : le dossier des modèles");
        let dir = PathBuf::from(models).join("parakeet_tdt_v3_int8");
        let source = dir.join("test_wavs").join("fr.wav");
        let wave = sherpa_onnx::Wave::read(source.to_str().unwrap()).expect("fr.wav illisible");
        let resampler =
            sherpa_onnx::LinearResampler::create(wave.sample_rate(), SAMPLE_RATE as i32).unwrap();
        let samples = resampler.resample(wave.samples(), true);
        let wav = scratch("reel").join("fr-16k.wav");
        write_wav(&wav, SAMPLE_RATE, 1, 32, true, &samples);

        let package = crate::models::CATALOG
            .iter()
            .find(|p| p.id == "parakeet_tdt_v3_int8")
            .unwrap();
        let engine = crate::stt::engine_for(package, 4).unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let text = dictate(
            chunks_of(&read_wav(&wav).unwrap()),
            engine,
            &dir,
            "fr",
            move |l| sink.lock().unwrap().push(l),
        )
        .unwrap();
        let lines = lines.lock().unwrap();
        eprintln!("texte : {text}\nprogression : {lines:#?}");
        let reference = "Ne vous demandez pas ce que votre pays peut faire pour vous. Demandez-vous plutôt ce que vous pouvez faire pour lui.";
        let error = crate::stt::wer::wer(reference, &text);
        assert!(error <= 0.1, "WER {error} : {text}");
        assert!(lines.iter().any(|l| l.starts_with("> ")), "{lines:?}");
    }
}

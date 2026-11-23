//! Banc de la dictée (porte du lot 4, tâche 10) : un audio, une référence,
//! un régime ; rend une ligne JSON. Rien n'est joué, aucun micro n'est ouvert.
//!
//! Régimes :
//! - `--detector silero|energy` : la découpe aux silences (`Dictation`,
//!   `Engine::Segmented`), comme une dictée ;
//! - `--full` : la relecture complète d'avant (tout l'audio depuis le début,
//!   au rythme de `Rolling`, puis une dernière lecture de tout l'audio à la
//!   fin demandée).
//!
//! `--realtime` donne l'audio au rythme du temps réel, par morceaux de
//! 100 ms ; sans lui, tout d'un coup : seuls le texte, le WER et le calcul
//! comptent alors (`delais_significatifs` : faux, délais rendus avec leur
//! seule `raison`). Après l'audio, 2 s de
//! silence numérique (`--tail-s`), puis la fin demandée : le dernier énoncé se
//! referme sur un silence, comme quand on se tait avant de relâcher la touche.
//!
//!   dictation_bench --model-dir …/parakeet_tdt_v3_int8 --runtime ort-cpu \
//!       --detector silero --silero …/silero_vad.onnx \
//!       --wav fr-pauses.wav --ref fr-pauses.txt --realtime
//!
//! Le binaire cherche sherpa-onnx et ONNX Runtime à côté de lui : voir
//! `scripts/dictee-porte.sh`, qui le construit et le lance comme
//! `scripts/ort-gate.sh`.
//!
//! Mesures :
//! - `premier_appel_s` : la première lecture après le chargement (les 2
//!   premières secondes de l'audio), avant la dictée ; le modèle reste
//!   chargé (`Holder`), comme d'une dictée à l'autre.
//! - `facteur_temps_reel` : le temps de calcul de toutes les lectures de la
//!   dictée (provisoires comprises) par seconde d'audio (R41).
//! - `delai_apres_coupe_s` : de l'arrivée de l'audio qui referme un énoncé à
//!   son texte définitif (`Event::Committed`).
//! - `delai_apres_fin_de_parole_s` : de la fin du dernier mot d'un énoncé
//!   refermé sur un silence à son texte définitif. La fin du mot vient des
//!   horodatages de Parakeet, lus après la mesure sur les mêmes énoncés (la
//!   découpe rejouée hors temps réel, avec un détecteur neuf du même type) :
//!   un repère acoustique, pas la décision du détecteur. En relecture
//!   complète : de la fin demandée au texte.
//! - `delai_par_tiers_s` : `delai_apres_coupe_s` des énoncés refermés dans le
//!   premier et le dernier tiers de l'audio.
//! - `pire_retard_provisoire_s` : le plus long moment où le texte montré
//!   (provisoire ou définitif) a du retard sur la parole : pour chaque
//!   lecture, sa fin moins le plus tard de deux instants, l'audio couvert par
//!   le texte précédent et le début de l'énoncé lu.
//! - `pic_mlx_mo` : toujours `null` ici, Parakeet sherpa-onnx calcule sur le
//!   processeur, sans MLX.

mod bench {
    use anyhow::{bail, ensure, Context, Result};
    use clap::Parser;
    use dysflow::stt::detector::{EnergyDetector, SpeechDetector};
    use dysflow::stt::dictation::{Dictation, Engine, Event, MIN_SAMPLES};
    use dysflow::stt::holder::Holder;
    use dysflow::stt::recognizer::Recognizer;
    use dysflow::stt::rolling::{Rolling, INITIAL};
    use dysflow::stt::segmenter::{Segment, Segmenter, MARGIN};
    use dysflow::stt::sherpa::parakeet::{ParakeetSherpa, FILES};
    use dysflow::stt::sherpa::vad::SileroDetector;
    use dysflow::stt::sherpa::VAD_SAMPLE_RATE;
    use dysflow::stt::wer::{wer, words};
    use dysflow::tts::Cancel;
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const RATE: f64 = VAD_SAMPLE_RATE as f64;

    #[derive(Parser)]
    #[command(about = "Banc de la dictée : un audio, une référence, un régime ; une ligne JSON")]
    struct Args {
        /// Le dossier de Parakeet int8 (parakeet_tdt_v3_int8/).
        #[arg(long)]
        model_dir: PathBuf,
        /// Le calcul : `ort-cpu` (sherpa-onnx sur le processeur). `mlx` vient
        /// avec le port de Parakeet (tâche 14).
        #[arg(long, default_value = "ort-cpu")]
        runtime: String,
        /// Le détecteur de la découpe : `silero` ou `energy`. Sans objet avec
        /// `--full`.
        #[arg(long)]
        detector: Option<String>,
        /// Le modèle de Silero (silero_vad.onnx), pour `--detector silero`.
        #[arg(long)]
        silero: Option<PathBuf>,
        /// L'audio : WAV mono 16 kHz.
        #[arg(long)]
        wav: PathBuf,
        /// Le texte lu dans l'audio.
        #[arg(long = "ref")]
        reference: PathBuf,
        /// L'audio au rythme du temps réel.
        #[arg(long)]
        realtime: bool,
        /// La relecture complète, au lieu de la découpe.
        #[arg(long)]
        full: bool,
        #[arg(long, default_value_t = 4)]
        threads: u32,
        #[arg(long, default_value = "fr")]
        language: String,
        /// Le silence numérique ajouté après l'audio, en secondes.
        #[arg(long, default_value_t = 2.0)]
        tail_s: f64,
    }

    /// Une lecture du modèle : son début, sa fin, sa longueur d'audio.
    #[derive(Clone, Copy)]
    struct Read {
        start: Instant,
        end: Instant,
        samples: usize,
    }

    /// Parakeet, chaque lecture chronométrée.
    struct Timed {
        inner: ParakeetSherpa,
        reads: Arc<Mutex<Vec<Read>>>,
    }

    impl Recognizer for Timed {
        fn load(&self, model_dir: &Path) -> Result<()> {
            self.inner.load(model_dir)
        }
        fn warm_up(&self) -> Result<()> {
            self.inner.warm_up()
        }
        fn transcribe(&self, samples: &[f32], language: &str, cancel: &Cancel) -> Result<String> {
            let start = Instant::now();
            let text = self.inner.transcribe(samples, language, cancel);
            let end = Instant::now();
            lock(&self.reads).push(Read {
                start,
                end,
                samples: samples.len(),
            });
            text
        }
        fn release(&self) {
            self.inner.release()
        }
    }

    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read_wav(path: &Path) -> Result<Vec<f32>> {
        let mut reader = hound::WavReader::open(path)
            .with_context(|| format!("{} illisible", path.display()))?;
        let spec = reader.spec();
        ensure!(
            spec.channels == 1 && spec.sample_rate == VAD_SAMPLE_RATE,
            "{} : il faut du mono à 16 kHz",
            path.display()
        );
        Ok(match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
            hound::SampleFormat::Int => {
                let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|s| s as f32 / scale))
                    .collect::<Result<_, _>>()?
            }
        })
    }

    fn detector(args: &Args) -> Result<Box<dyn SpeechDetector>> {
        match args.detector.as_deref() {
            Some("energy") => Ok(Box::new(EnergyDetector::new())),
            Some("silero") => {
                let model = args
                    .silero
                    .as_deref()
                    .context("--silero : le modèle de Silero")?;
                Ok(Box::new(SileroDetector::new(model)?))
            }
            other => bail!("--detector silero ou energy (reçu {other:?})"),
        }
    }

    /// Médiane, 95e centile (rang le plus proche), plus grand, en secondes.
    fn stats(values: &[f64]) -> Value {
        if values.is_empty() {
            return json!({ "n": 0, "p50": null, "p95": null, "max": null });
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let rank =
            |p: f64| sorted[((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1];
        json!({
            "n": sorted.len(),
            "p50": round(rank(0.50)),
            "p95": round(rank(0.95)),
            "max": round(sorted[sorted.len() - 1]),
        })
    }

    fn round(x: f64) -> f64 {
        (x * 1000.0).round() / 1000.0
    }

    fn secs(d: Duration) -> f64 {
        d.as_secs_f64()
    }

    /// Mémoire résidente maximale du processus, en Mo.
    #[cfg(unix)]
    fn max_rss_mb() -> Option<f64> {
        // SAFETY : `getrusage` remplit la structure passée, rien d'autre.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
            return None;
        }
        // Octets sous macOS, kilo-octets sous Linux.
        let bytes = if cfg!(target_os = "macos") {
            usage.ru_maxrss as f64
        } else {
            usage.ru_maxrss as f64 * 1024.0
        };
        Some((bytes / 1e6).round())
    }

    #[cfg(not(unix))]
    fn max_rss_mb() -> Option<f64> {
        None
    }

    /// La charge sur 1, 5, 15 minutes et le swap utilisé (Mo).
    fn machine() -> Value {
        json!({ "charge": load_average(), "swap_mo": swap_mb() })
    }

    #[cfg(unix)]
    fn load_average() -> Vec<f64> {
        let mut loads = [0f64; 3];
        // SAFETY : `getloadavg` écrit au plus 3 valeurs dans le tableau.
        let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
        loads[..n.clamp(0, 3) as usize]
            .iter()
            .map(|l| round(*l))
            .collect()
    }

    #[cfg(not(unix))]
    fn load_average() -> Vec<f64> {
        Vec::new()
    }

    fn swap_mb() -> Option<f64> {
        if cfg!(target_os = "macos") {
            let out = std::process::Command::new("sysctl")
                .args(["-n", "vm.swapusage"])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            let used = text.split("used = ").nth(1)?.split('M').next()?;
            used.trim().replace(',', ".").parse().ok()
        } else {
            let info = std::fs::read_to_string("/proc/meminfo").ok()?;
            let field = |name: &str| -> Option<f64> {
                let line = info.lines().find(|l| l.starts_with(name))?;
                line.split_whitespace().nth(1)?.parse().ok()
            };
            Some(((field("SwapTotal:")? - field("SwapFree:")?) / 1024.0).round())
        }
    }

    /// L'audio part au rythme du temps réel (ou tout d'un coup) : `push` reçoit
    /// chaque morceau de 100 ms à l'heure où il finit d'être « dit ».
    fn feed(audio: &[f32], t0: Instant, realtime: bool, mut push: impl FnMut(&[f32])) {
        let chunk = VAD_SAMPLE_RATE as usize / 10;
        for (i, part) in audio.chunks(chunk).enumerate() {
            if realtime {
                let due = t0 + Duration::from_secs_f64(((i * chunk + part.len()) as f64) / RATE);
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
            push(part);
        }
    }

    /// Une mise à jour du texte montré : quand, l'instant de la parole
    /// qu'elle couvre, et l'instant où commence l'audio lu.
    struct Shown {
        at: Instant,
        covers: Instant,
        begins: Instant,
    }

    /// Voir `pire_retard_provisoire_s` en tête du fichier.
    fn worst_lag(shown: &[Shown]) -> Option<f64> {
        shown
            .windows(2)
            .map(|w| {
                secs(
                    w[1].at
                        .saturating_duration_since(w[0].covers.max(w[1].begins)),
                )
            })
            .fold(None, |worst: Option<f64>, lag| {
                Some(worst.map_or(lag, |w| w.max(lag)))
            })
            .map(round)
    }

    struct Outcome {
        text: String,
        reads: Vec<Read>,
        finals: usize,
        final_compute: f64,
        cut_delays: Vec<f64>,
        thirds: Value,
        /// Segmenté : l'heure de chaque texte définitif, en secondes depuis
        /// le début de l'audio.
        commits: Vec<f64>,
        /// Relecture complète : `delai_apres_fin_de_parole_s`.
        speech_end: Value,
        lag: Option<f64>,
        failures: Vec<String>,
    }

    pub fn main() -> Result<()> {
        let args = Args::parse();
        if args.runtime != "ort-cpu" {
            bail!(
                "--runtime {} : seul ort-cpu existe (Parakeet en MLX : tâche 14)",
                args.runtime
            );
        }
        if args.full == args.detector.is_some() {
            bail!("soit --full, soit --detector silero|energy");
        }
        for file in FILES {
            ensure!(
                args.model_dir.join(file).is_file(),
                "{} absent",
                args.model_dir.join(file).display()
            );
        }
        let reference = std::fs::read_to_string(&args.reference)
            .with_context(|| format!("{} illisible", args.reference.display()))?;
        let spoken = read_wav(&args.wav)?;
        let audio_s = spoken.len() as f64 / RATE;
        let mut audio = spoken.clone();
        audio.resize(spoken.len() + (args.tail_s * RATE) as usize, 0.0);
        let start_machine = machine();
        let wall = Instant::now();

        let reads = Arc::new(Mutex::new(Vec::new()));
        let recognizer = Arc::new(Holder::new(
            Timed {
                inner: ParakeetSherpa::new(args.threads),
                reads: reads.clone(),
            },
            None,
        ));
        let loaded = Instant::now();
        recognizer.prepare(&args.model_dir, &|| {})?;
        let load_s = secs(loaded.elapsed());
        let first = &audio[..audio.len().min(2 * VAD_SAMPLE_RATE as usize)];
        let first_call = Instant::now();
        recognizer.transcribe(first, &args.language, &Cancel::never())?;
        let first_call_s = secs(first_call.elapsed());
        lock(&reads).clear();

        let outcome = if args.full {
            run_full(&args, &audio, &recognizer, &reads)?
        } else {
            run_segmented(&args, &audio, audio_s, &recognizer, &reads)?
        };
        let rss = max_rss_mb();
        let end_machine = machine();
        let wall_s = secs(wall.elapsed());

        let compute: f64 = outcome.reads.iter().map(|r| secs(r.end - r.start)).sum();
        let (speech_end, segments) = if args.full {
            (
                outcome.speech_end.clone(),
                json!({ "lus": 1, "coupes_forcees": 0 }),
            )
        } else {
            speech_end_delays(&args, &audio, &outcome.commits)?
        };
        let heard = words(&outcome.text);
        let expected = words(&reference);
        let mut report = json!({
            "audio": args.wav.display().to_string(),
            "secondes_audio": round(audio_s),
            "silence_ajoute_s": args.tail_s,
            "mots_reference": expected.len(),
            "moteur": { "modele": "parakeet_tdt_v3_int8", "runtime": args.runtime, "fils": args.threads.clamp(1, 4) },
            "regime": if args.full { "relecture complète" } else { "découpe" },
            "detecteur": args.detector,
            "temps_reel": args.realtime,
            "delais_significatifs": args.realtime,
            "chargement_s": round(load_s),
            "premier_appel_s": round(first_call_s),
            "facteur_temps_reel": round(compute / audio_s),
            "calcul_s": {
                "total": round(compute),
                "definitif": round(outcome.final_compute),
                "provisoire": round(compute - outcome.final_compute),
            },
            "lectures": { "definitives": outcome.finals, "provisoires": outcome.reads.len() - outcome.finals },
            "pic_mlx_mo": null,
            "pic_mlx_raison": "Parakeet sherpa-onnx calcule sur le processeur, sans MLX",
            "wer": round(wer(&reference, &outcome.text)),
            "reference_entendue": !expected.is_empty() && expected.iter().all(|w| heard.contains(w)),
            "texte": outcome.text,
            "segments": segments,
            "delai_apres_coupe_s": if args.full { Value::Null } else { stats(&outcome.cut_delays) },
            "delai_apres_fin_de_parole_s": speech_end,
            "delai_par_tiers_s": outcome.thirds,
            "pire_retard_provisoire_s": outcome.lag,
            "echecs": outcome.failures,
            "rss_pic_mo": rss,
            "machine": { "debut": start_machine, "fin": end_machine },
            "duree_murale_s": round(wall_s),
        });
        if !args.realtime {
            // Tout l'audio arrive d'un coup : un « délai » mesurerait la file
            // d'attente, pas la dictée. Les champs restent, sans valeur.
            let why =
                json!({ "raison": "sans --realtime, l'audio arrive d'un coup : délai sans objet" });
            for field in [
                "delai_apres_coupe_s",
                "delai_apres_fin_de_parole_s",
                "delai_par_tiers_s",
            ] {
                report[field] = why.clone();
            }
            report["pire_retard_provisoire_s"] = Value::Null;
        }
        println!("{report}");
        Ok(())
    }

    fn run_segmented(
        args: &Args,
        audio: &[f32],
        audio_s: f64,
        recognizer: &Arc<Holder<Timed>>,
        reads: &Arc<Mutex<Vec<Read>>>,
    ) -> Result<Outcome> {
        let events: Arc<Mutex<Vec<(Instant, Event)>>> = Arc::default();
        let sink = events.clone();
        let t0 = Instant::now();
        let dictation = Dictation::start(
            Engine::Segmented(recognizer.clone(), detector(args)?),
            &args.model_dir,
            &args.language,
            move |event| lock(&sink).push((Instant::now(), event)),
        );
        feed(audio, t0, args.realtime, |part| dictation.push(part));
        let text = dictation.finish()?;
        let reads = lock(reads).clone();
        let events = lock(&events).clone();

        // Chaque texte définitif suit la lecture qui l'a produit.
        let mut finals = Vec::new();
        let mut failures = Vec::new();
        for (at, event) in &events {
            match event {
                Event::Committed { delay, .. } => {
                    let read = reads.iter().rposition(|r| r.end <= *at);
                    finals.push((*at, *delay, read));
                }
                Event::Failed(message) => failures.push(message.clone()),
                _ => {}
            }
        }
        let final_reads: Vec<usize> = finals.iter().filter_map(|f| f.2).collect();
        let final_compute = final_reads
            .iter()
            .map(|&i| secs(reads[i].end - reads[i].start))
            .sum();

        let mut shown = Vec::new();
        for (i, read) in reads.iter().enumerate() {
            let closed = finals.iter().find(|f| f.2 == Some(i));
            let covers = closed.map_or(read.start, |(at, delay, _)| *at - *delay);
            let span = Duration::from_secs_f64(read.samples.saturating_sub(MARGIN) as f64 / RATE);
            shown.push(Shown {
                at: read.end,
                covers,
                begins: covers.checked_sub(span).unwrap_or(covers),
            });
        }

        let cut_delays: Vec<f64> = finals.iter().map(|f| secs(f.1)).collect();
        let third = |range: std::ops::Range<f64>| -> Value {
            let delays: Vec<f64> = finals
                .iter()
                .filter(|(at, delay, _)| {
                    let closed = secs((*at - *delay).saturating_duration_since(t0)) / audio_s;
                    range.contains(&closed)
                })
                .map(|f| secs(f.1))
                .collect();
            stats(&delays)
        };
        let thirds = json!({
            "premier": third(0.0..1.0 / 3.0),
            "dernier": third(2.0 / 3.0..f64::INFINITY),
        });
        Ok(Outcome {
            text,
            finals: final_reads.len(),
            final_compute,
            cut_delays,
            thirds,
            commits: finals
                .iter()
                .map(|f| secs(f.0.saturating_duration_since(t0)))
                .collect(),
            speech_end: Value::Null,
            lag: if args.realtime {
                worst_lag(&shown)
            } else {
                None
            },
            reads,
            failures,
        })
    }

    /// La découpe rejouée hors temps réel (même audio, détecteur neuf du même
    /// type), puis la fin du dernier mot de chaque énoncé lue par Parakeet
    /// (horodatages), pour `delai_apres_fin_de_parole_s`.
    /// Rend ce délai et le compte des énoncés.
    fn speech_end_delays(args: &Args, audio: &[f32], commits: &[f64]) -> Result<(Value, Value)> {
        let mut segmenter = Segmenter::new(detector(args)?);
        let mut segments: Vec<Segment> = segmenter.push(audio)?;
        segments.extend(segmenter.finish()?);
        let read: Vec<&Segment> = segments
            .iter()
            .filter(|s| s.samples.len() >= MIN_SAMPLES)
            .collect();
        let forced = read.iter().filter(|s| s.forced).count();
        let counts = json!({ "lus": read.len(), "coupes_forcees": forced });
        let mut out = json!({
            "reference": "fin du dernier mot (horodatages de Parakeet) d'un énoncé refermé sur un silence",
        });
        if read.len() != commits.len() {
            out["raison"] = json!(format!(
                "la découpe rejouée rend {} énoncés, la dictée {} textes définitifs : délais non appariés",
                read.len(),
                commits.len()
            ));
            out["mesure"] = stats(&[]);
            return Ok((out, counts));
        }
        let recognizer = timestamps_recognizer(args)?;
        let mut delays = Vec::new();
        let mut without_words = 0;
        for (segment, committed_at) in read.iter().zip(commits) {
            if segment.forced {
                continue;
            }
            let stream = recognizer.create_stream();
            stream.accept_waveform(VAD_SAMPLE_RATE as i32, &segment.samples);
            recognizer.decode(&stream);
            let result = stream
                .get_result()
                .context("Parakeet ne rend aucun résultat")?;
            let last_end = match (result.timestamps.as_deref(), result.durations.as_deref()) {
                (Some(ts), Some(ds)) if !ts.is_empty() && ds.len() == ts.len() => {
                    Some((ts[ts.len() - 1] + ds[ds.len() - 1]) as f64)
                }
                // Sans durées, la fin du dernier jeton : une trame de
                // l'encodeur (80 ms) après son début.
                (Some(ts), _) if !ts.is_empty() => Some(ts[ts.len() - 1] as f64 + 0.08),
                _ => None,
            };
            let Some(last_end) = last_end else {
                without_words += 1;
                continue;
            };
            let spoken_until = segment.start as f64 / RATE + last_end;
            delays.push(committed_at - spoken_until);
        }
        out["sans_mot"] = json!(without_words);
        out["mesure"] = stats(&delays);
        Ok((out, counts))
    }

    /// Un second Parakeet, celui de `ParakeetSherpa` (mêmes fichiers, même
    /// calcul), dont on lit les horodatages ; créé après la mesure.
    fn timestamps_recognizer(args: &Args) -> Result<sherpa_onnx::OfflineRecognizer> {
        let path = |file: &str| Some(args.model_dir.join(file).display().to_string());
        let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
        config.model_config.transducer = sherpa_onnx::OfflineTransducerModelConfig {
            encoder: path(FILES[0]),
            decoder: path(FILES[1]),
            joiner: path(FILES[2]),
        };
        config.model_config.tokens = path(FILES[3]);
        config.model_config.model_type = Some("nemo_transducer".to_string());
        config.model_config.num_threads = args.threads.clamp(1, 4) as i32;
        config.model_config.provider = Some("cpu".to_string());
        sherpa_onnx::OfflineRecognizer::create(&config)
            .context("sherpa-onnx ne charge pas Parakeet")
    }

    /// La relecture complète : tout l'audio arrivé, relu au rythme de
    /// `Rolling` ; à la fin demandée, une dernière lecture de tout l'audio.
    fn run_full(
        args: &Args,
        audio: &[f32],
        recognizer: &Arc<Holder<Timed>>,
        reads: &Arc<Mutex<Vec<Read>>>,
    ) -> Result<Outcome> {
        let heard: Arc<Mutex<(Vec<f32>, bool)>> = Arc::default();
        let t0 = Instant::now();
        let failures = Vec::new();
        let (text, finish_at) = std::thread::scope(|scope| -> Result<(String, Instant)> {
            let writer = heard.clone();
            scope.spawn(move || {
                feed(audio, t0, args.realtime, |part| {
                    lock(&writer).0.extend_from_slice(part)
                });
                lock(&writer).1 = true;
            });
            let mut rolling = Rolling::new(INITIAL);
            let mut read_len = 0;
            loop {
                let (snapshot, done) = {
                    let state = lock(&heard);
                    if state.1 {
                        (Vec::new(), true)
                    } else if state.0.len() >= MIN_SAMPLES
                        && state.0.len() > read_len
                        && rolling.should_read(t0.elapsed(), false)
                    {
                        (state.0.clone(), false)
                    } else {
                        (Vec::new(), false)
                    }
                };
                if done {
                    break;
                }
                if snapshot.is_empty() {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                read_len = snapshot.len();
                let start = Instant::now();
                // Un échec d'une lecture provisoire ne dit rien, comme dans
                // la dictée.
                let _ = recognizer.transcribe(&snapshot, &args.language, &Cancel::never());
                rolling.adapt(start.elapsed());
            }
            let finish_at = Instant::now();
            let all = lock(&heard).0.clone();
            let text = recognizer.transcribe(&all, &args.language, &Cancel::never())?;
            Ok((text, finish_at))
        })?;
        let reads = lock(reads).clone();
        let last = *reads.last().context("aucune lecture")?;
        let shown: Vec<Shown> = reads
            .iter()
            .enumerate()
            .map(|(i, r)| Shown {
                at: r.end,
                covers: if i + 1 == reads.len() {
                    finish_at
                } else {
                    r.start
                },
                begins: t0,
            })
            .collect();
        Ok(Outcome {
            text: text.trim().to_string(),
            finals: 1,
            final_compute: secs(last.end - last.start),
            cut_delays: Vec::new(),
            thirds: Value::Null,
            speech_end: json!({
                "reference": "la fin demandée (touche relâchée), après le silence ajouté",
                "mesure": stats(&[secs(last.end.saturating_duration_since(finish_at))]),
            }),
            commits: Vec::new(),
            lag: if args.realtime {
                worst_lag(&shown)
            } else {
                None
            },
            reads,
            failures,
        })
    }
}

fn main() -> anyhow::Result<()> {
    bench::main()
}

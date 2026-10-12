//! Phonétisation par espeak-ng, lancé comme programme séparé : la GPL-3.0
//! d'espeak-ng reste de son côté. Le traitement de misaki (`EspeakG2P` pour
//! le français, `EspeakFallback` pour l'anglais) est porté tel quel : Kokoro
//! a été entraîné sur ces phonèmes.
//!
//! Protocole : une ligne de texte, puis une ligne sentinelle ; tout ce que
//! espeak-ng écrit avant la réponse connue de la sentinelle appartient à la
//! ligne. Aucun texte n'est jamais journalisé.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::LazyLock;
use std::time::Duration;

/// Ce que la synthèse demande à la phonétisation.
pub trait Phonemizer: Send {
    /// Les phonèmes de chaque entrée, dans l'ordre, en une seule requête.
    fn phonemize(&mut self, language: &str, items: &[String]) -> Result<Vec<String>>;
    /// Arrête ce qui tourne : la voix est relâchée.
    fn stop(&mut self) {}
}

/// Une phrase qu'aucun texte ne contient, dont la réponse marque la fin.
pub(crate) const SENTINEL: &str = "Zyzomys.";
/// Au-delà, espeak-ng est tenu pour bloqué.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);
/// La ponctuation que misaki garde (phonemizer, `preserve_punctuation`).
const MARKS: &[char] = &[';', ':', ',', '.', '!', '?', '¡', '¿', '—', '…', '"', '«', '»', '“', '”'];

/// La voix espeak-ng d'une langue.
pub(crate) fn voice_of(language: &str) -> &'static str {
    match crate::text::language::Lang::parse(language) {
        Some(crate::text::language::Lang::En) => "en-us",
        _ => "fr-fr",
    }
}

/// Le programme espeak-ng : `SILLON_ESPEAK` (l'app y met le sien), à côté du
/// binaire, puis le `PATH`.
pub fn locate_program() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SILLON_ESPEAK").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("espeak-ng")));
    if let Some(path) = beside.filter(|p| p.is_file()) {
        return Some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|d| d.join("espeak-ng")).find(|p| p.is_file())
}

/// Le dossier qui contient `espeak-ng-data`, quand il n'est pas celui du
/// système (l'app : `Contents/Resources`).
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SILLON_ESPEAK_DATA").map(PathBuf::from)
}

/// Un morceau de texte entre deux ponctuations, ou la ponctuation elle-même
/// avec ses espaces.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Marks(String),
}

/// Prépare le texte comme misaki : guillemets français en guillemets
/// courbes, parenthèses en chevrons (que phonemizer garde), retours à la
/// ligne en espaces.
fn prepare(text: &str) -> String {
    text.replace('«', "\u{201c}")
        .replace('»', "\u{201d}")
        .replace('(', "«")
        .replace(')', "»")
        .replace(['\n', '\r'], " ")
}

/// Découpe aux ponctuations : un groupe de ponctuations et d'espaces qui
/// contient au moins un signe est gardé tel quel.
fn split_punctuation(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut text_run = String::new();
    let mut gap = String::new();
    let mut gap_has_mark = false;
    for c in text.chars() {
        if MARKS.contains(&c) || c.is_whitespace() {
            gap.push(c);
            gap_has_mark |= MARKS.contains(&c);
            continue;
        }
        if !gap.is_empty() {
            if gap_has_mark {
                if !text_run.trim().is_empty() {
                    pieces.push(Piece::Text(text_run.trim().to_string()));
                }
                text_run.clear();
                pieces.push(Piece::Marks(std::mem::take(&mut gap)));
            } else {
                text_run.push_str(&gap);
                gap.clear();
            }
            gap_has_mark = false;
        }
        text_run.push(c);
    }
    if !text_run.trim().is_empty() {
        pieces.push(Piece::Text(text_run.trim().to_string()));
    }
    if gap_has_mark {
        pieces.push(Piece::Marks(gap));
    }
    pieces
}

/// Remplacements de misaki pour le français (`EspeakG2P`, version 1.0).
const FR_E2M: &[(&str, &str)] = &[
    ("a^ɪ", "I"), ("a^ʊ", "W"), ("d^z", "ʣ"), ("d^ʒ", "ʤ"), ("e^ɪ", "A"), ("o^ʊ", "O"),
    ("s^s", "S"), ("t^s", "ʦ"), ("t^ʃ", "ʧ"), ("ɔ^ɪ", "Y"), ("ə^ʊ", "Q"),
];

/// Remplacements de misaki pour l'anglais (`EspeakFallback`), du plus long
/// au plus court, dans l'ordre de misaki.
const EN_E2M: &[(&str, &str)] = &[
    ("ʔˌn\u{329}", "ʔn"),
    ("ʔn\u{329}", "ʔn"), ("a^ɪ", "I"), ("a^ʊ", "W"), ("d^ʒ", "ʤ"), ("e^ɪ", "A"), ("t^ʃ", "ʧ"),
    ("ɔ^ɪ", "Y"), ("ə^l", "ᵊl"),
    ("ʲo", "jo"), ("ʲə", "jə"),
    ("e", "A"), ("ʲ", ""), ("ɚ", "əɹ"), ("r", "ɹ"), ("x", "k"), ("ç", "k"), ("ɐ", "ə"),
    ("ɬ", "l"), ("\u{303}", ""),
];

/// La sortie brute d'espeak-ng mise aux symboles de Kokoro, comme misaki.
pub(crate) fn finish(language: &str, raw: &str) -> String {
    // Liens d'espeak-ng (U+0361) en « ^ », drapeaux de langue « (en) » retirés
    // (avec `--tie`, leurs lettres sont liées aussi : « (^e^n) »).
    static FLAGS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\((?:\^?[a-z]){2,3}(?:\^?-(?:\^?[a-z0-9])+)?\)").expect("motif fixe")
    });
    static SYLLABIC: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(\S)\u{329}").expect("motif fixe"));
    let mut ps = raw.replace('\u{361}', "^");
    ps = FLAGS.replace_all(&ps, "").into_owned();
    ps = ps.split_whitespace().collect::<Vec<_>>().join(" ");
    if voice_of(language) == "en-us" {
        for (old, new) in EN_E2M {
            ps = ps.replace(old, new);
        }
        ps = SYLLABIC.replace_all(&ps, "ᵊ$1").into_owned().replace('\u{329}', "");
        ps = ps.replace("o^ʊ", "O").replace("ɜːɹ", "ɜɹ").replace("ɜː", "ɜɹ").replace("ɪə", "iə").replace('ː', "");
        ps = ps.replace('o', "ɔ");
        ps = ps.replace('ɾ', "T").replace('ʔ', "t");
        ps = ps.replace('^', "");
    } else {
        for (old, new) in FR_E2M {
            ps = ps.replace(old, new);
        }
        ps = ps.replace(['^', '-'], "");
    }
    ps.replace('«', "(").replace('»', ")").trim().to_string()
}

struct Running {
    process: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    sentinel: String,
}

impl Running {
    fn read_line(&self) -> Result<String> {
        self.lines.recv_timeout(ANSWER_TIMEOUT).map_err(|_| anyhow!("espeak-ng ne répond plus"))
    }

    /// Une requête : chaque ligne suivie de la sentinelle, une réponse par
    /// ligne.
    fn ask(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let mut request = String::new();
        for line in lines {
            request.push_str(line);
            request.push('\n');
            request.push_str(SENTINEL);
            request.push('\n');
        }
        self.stdin.write_all(request.as_bytes()).context("écriture vers espeak-ng")?;
        self.stdin.flush().context("écriture vers espeak-ng")?;
        let mut answers = Vec::with_capacity(lines.len());
        for _ in lines {
            let mut parts = Vec::new();
            loop {
                let line = self.read_line()?;
                if line.trim() == self.sentinel {
                    break;
                }
                parts.push(line.trim().to_string());
            }
            answers.push(parts.join(" "));
        }
        Ok(answers)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// espeak-ng, un processus par langue, lancé à la demande.
pub struct EspeakProcess {
    program: PathBuf,
    data: Option<PathBuf>,
    running: HashMap<&'static str, Running>,
}

impl EspeakProcess {
    pub fn new(program: PathBuf, data: Option<PathBuf>) -> Self {
        Self { program, data, running: HashMap::new() }
    }

    /// Le programme trouvé, ou la raison de son absence.
    pub fn locate() -> Result<Self> {
        match locate_program() {
            Some(program) => Ok(Self::new(program, data_dir())),
            None if std::env::var_os("SILLON_ESPEAK").is_some() => {
                bail!("Installation de Sillon incomplète (espeak-ng)")
            }
            None => bail!("espeak-ng introuvable — installez le paquet `espeak-ng`"),
        }
    }

    fn spawn(&self, voice: &str) -> Result<Running> {
        let mut command = Command::new(&self.program);
        // `-q` n'empêche pas espeak-ng de calculer le son de chaque ligne, puis
        // de le jeter : au débit le plus haut (450 mots par minute), ce calcul
        // coûte deux fois moins, et les phonèmes restent les mêmes (vérifié
        // sur 2 397 lignes françaises et anglaises).
        command.args(["-q", "--ipa", "--tie", "-s", "450", "-v", voice]);
        if let Some(data) = &self.data {
            command.arg(format!("--path={}", data.display()));
        }
        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("lancement de {}", self.program.display()))?;
        let stdin = process.stdin.take().context("entrée d'espeak-ng")?;
        let stdout = process.stdout.take().context("sortie d'espeak-ng")?;
        let (tx, lines) = mpsc::channel();
        std::thread::Builder::new()
            .name("espeak-ng".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .context("fil de lecture d'espeak-ng")?;
        let mut running = Running { process, stdin, lines, sentinel: String::new() };
        // La réponse de la sentinelle, apprise une fois.
        running.stdin.write_all(format!("{SENTINEL}\n").as_bytes())?;
        running.stdin.flush()?;
        running.sentinel = running.read_line()?.trim().to_string();
        Ok(running)
    }

    fn ask(&mut self, voice: &'static str, lines: &[String]) -> Result<Vec<String>> {
        if !self.running.contains_key(voice) {
            let running = self.spawn(voice)?;
            self.running.insert(voice, running);
        }
        let running = self.running.get_mut(voice).expect("lancé juste au-dessus");
        let outcome = running.ask(lines);
        if outcome.is_err() {
            self.running.remove(voice);
        }
        outcome
    }
}

impl Phonemizer for EspeakProcess {
    fn phonemize(&mut self, language: &str, items: &[String]) -> Result<Vec<String>> {
        let voice = voice_of(language);
        let pieces: Vec<Vec<Piece>> = items.iter().map(|t| split_punctuation(&prepare(t))).collect();
        let lines: Vec<String> = pieces
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Piece::Text(t) => Some(t.clone()),
                Piece::Marks(_) => None,
            })
            .collect();
        let answers = if lines.is_empty() {
            Vec::new()
        } else {
            // Une relance, pas deux : un programme qui meurt deux fois de
            // suite est en panne, et la voix passe en repli.
            match self.ask(voice, &lines) {
                Ok(answers) => answers,
                Err(_) => self.ask(voice, &lines)?,
            }
        };
        let mut answers = answers.into_iter();
        Ok(pieces
            .iter()
            .map(|item| {
                let mut raw = String::new();
                for piece in item {
                    match piece {
                        Piece::Text(_) => raw.push_str(&answers.next().unwrap_or_default()),
                        Piece::Marks(m) => raw.push_str(m),
                    }
                }
                finish(language, &raw)
            })
            .collect())
    }

    fn stop(&mut self) {
        self.running.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Un faux espeak-ng : la sentinelle a sa réponse, toute autre ligne
    /// répond sur deux lignes (`<texte>` puis `x`). Avec `die_once`, la
    /// ligne `meurs` tue le processus la première fois seulement ; avec
    /// `die_always`, à chaque fois.
    fn fake(name: &str, die_once: bool, die_always: bool) -> EspeakProcess {
        let dir = std::env::temp_dir().join(format!("sillon-espeak-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("deja-mort");
        let _ = std::fs::remove_file(&marker);
        let script = dir.join("espeak-ng");
        let death = if die_always {
            "exit 1".to_string()
        } else if die_once {
            format!("if [ ! -f '{m}' ]; then : > '{m}'; exit 1; fi", m = marker.display())
        } else {
            ":".to_string()
        };
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nwhile IFS= read -r line; do\n  if [ \"$line\" = \"{SENTINEL}\" ]; then echo 'zizomis'; continue; fi\n  if [ \"$line\" = \"meurs\" ]; then {death}; fi\n  echo \"<$line>\"; echo x\ndone\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        EspeakProcess::new(script, None)
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_sentinel_separates_answers_spread_over_several_lines() {
        let mut espeak = fake("lignes", false, false);
        let out = espeak.phonemize("fr", &strings(&["bonjour", "le monde"])).unwrap();
        assert_eq!(out, ["<bonjour> x", "<le monde> x"]);
        // Un second aller-retour sur le même processus.
        assert_eq!(espeak.phonemize("fr", &strings(&["encore"])).unwrap(), ["<encore> x"]);
    }

    #[test]
    fn punctuation_stays_where_it_was() {
        let mut espeak = fake("ponctuation", false, false);
        let out = espeak.phonemize("fr", &strings(&["Peut-être : les notes, et (l'ordre)."])).unwrap();
        // Les morceaux entre ponctuations partent seuls ; « - » disparaît
        // comme chez misaki, et les parenthèses reviennent.
        assert_eq!(out, ["<Peutêtre> x : <les notes> x, <et> x (<l'ordre> x)."]);
    }

    #[test]
    fn an_empty_item_gives_an_empty_answer() {
        let mut espeak = fake("vide", false, false);
        assert_eq!(espeak.phonemize("fr", &strings(&["", "."])).unwrap(), ["", "."]);
    }

    #[test]
    fn a_dead_process_is_restarted_once() {
        let mut espeak = fake("relance", true, false);
        assert_eq!(espeak.phonemize("fr", &strings(&["meurs"])).unwrap(), ["<meurs> x"]);
        let mut doomed = fake("condamne", false, true);
        assert!(doomed.phonemize("fr", &strings(&["meurs"])).is_err(), "deux morts : échec");
        assert_eq!(doomed.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant> x"], "on repart");
    }

    #[test]
    fn ties_and_flags_are_turned_into_kokoro_symbols() {
        // espeak-ng écrit les liens en U+0361 ; misaki les note « ^ ».
        assert_eq!(finish("fr", "ad\u{361}ʒɛ-ktif"), "aʤɛktif");
        assert_eq!(finish("fr", "t\u{361}ʃˈa (en)ʃˈuː(fr)"), "ʧˈa ʃˈuː");
        // Avec `--tie`, espeak-ng lie aussi les lettres des drapeaux.
        assert_eq!(finish("fr", "(\u{361}e\u{361}n)zˈa\u{361}ɪzəmiz(\u{361}f\u{361}r)"), "zˈIzəmiz");
        assert_eq!(finish("en", "hə\u{2c8}lo\u{361}ʊ ɐ"), "hə\u{2c8}lO ə");
        assert_eq!(finish("en", "ɹˈɑːdɚ"), "ɹˈɑdəɹ");
    }

    #[test]
    fn each_language_has_its_espeak_voice() {
        assert_eq!(voice_of("fr"), "fr-fr");
        assert_eq!(voice_of("en-GB"), "en-us");
    }

    /// Parité avec misaki : `SILLON_ESPEAK=target/espeak/bin/espeak-ng
    /// SILLON_ESPEAK_DATA=target/espeak/share cargo test --lib kokoro::phonemes -- --ignored`
    #[test]
    #[ignore]
    fn french_phonemes_match_misaki() {
        let reference: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/misaki-phonemes.json")).unwrap();
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let fr = &reference["fr"];
        let texts: Vec<String> = fr.keys().cloned().collect();
        let ours = espeak.phonemize("fr", &texts).unwrap();
        for (text, mine) in texts.iter().zip(&ours) {
            assert_eq!(mine, &fr[text], "écart sur une phrase de référence");
        }
    }

    /// L'anglais de misaki passe d'abord par un lexique : espeak-ng seul
    /// s'en approche sans l'égaler. On mesure l'écart, on le borne.
    #[test]
    #[ignore]
    fn english_phonemes_stay_close_to_misaki() {
        let reference: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/misaki-phonemes.json")).unwrap();
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let en = &reference["en"];
        let texts: Vec<String> = en.keys().cloned().collect();
        let ours = espeak.phonemize("en", &texts).unwrap();
        let mut total = 0.0;
        for (text, mine) in texts.iter().zip(&ours) {
            total += similarity(mine, &en[text]);
        }
        let mean = total / texts.len() as f64;
        eprintln!("anglais : similarité moyenne avec misaki {mean:.3}");
        assert!(mean >= 0.80, "{mean}");
    }

    fn similarity(a: &str, b: &str) -> f64 {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut prev = row[0];
            row[0] = i;
            for j in 1..=b.len() {
                let cur = row[j];
                row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
                prev = cur;
            }
        }
        1.0 - row[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
    }

    /// Budget : ≤ 5 ms par phrase, phrase et mots seuls compris, à chaud.
    #[test]
    #[ignore]
    fn a_sentence_and_its_words_cost_under_five_milliseconds() {
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let sentence = "Le rapport de la SNCF, long de trente-sept pages, est arrivé un mardi.";
        let mut items = vec![sentence.to_string()];
        items.extend(sentence.split_whitespace().map(str::to_string));
        espeak.phonemize("fr", &items).unwrap();
        let mut times = Vec::new();
        for _ in 0..20 {
            let t = std::time::Instant::now();
            espeak.phonemize("fr", &items).unwrap();
            times.push(t.elapsed());
        }
        times.sort();
        eprintln!("phonétisation : médiane {:?}", times[10]);
        assert!(times[10] <= std::time::Duration::from_millis(5), "{:?}", times[10]);
    }
}

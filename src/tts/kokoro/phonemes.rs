//! Phonétisation par espeak-ng, par `dysflow-espeak` : un petit programme à
//! nous (GPL-3.0 comme espeak-ng, `scripts/dysflow-espeak.c`), lié à
//! libespeak-ng et lancé comme processus séparé. Dysflow ne se lie pas à
//! espeak-ng : la GPL-3.0 reste de son côté. Le traitement de misaki
//! (`EspeakG2P` pour le français, `EspeakFallback` pour l'anglais) est porté
//! tel quel : Kokoro a été entraîné sur ces phonèmes.
//!
//! Protocole binaire : chaque trame est précédée de sa longueur, et chaque
//! entrée d'un lot reçoit exactement une trame de réponse, puis le lot finit
//! par « ok ». Le découpage ne tient qu'aux longueurs : aucun texte ne peut
//! le déplacer. Aucun texte n'est jamais journalisé.

use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

/// La plus longue ligne envoyée : un morceau plus long est coupé, pour
/// borner la taille d'un lot.
const MAX_LINE: usize = 900;
/// Un lot tient dans le tampon d'un tube vide (16 Kio sous macOS) : l'écrire
/// ne bloque jamais, même si le programme ne lit plus.
const MAX_BATCH: usize = 8 * 1024;
/// La plus longue trame de réponse acceptée (les phonèmes d'une ligne de
/// `MAX_LINE` octets en font bien moins).
const MAX_FRAME: usize = 64 * 1024;
/// Au-delà, le programme est tenu pour bloqué.
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

/// Le dossier du binaire, liens symboliques résolus : `$PREFIX/bin/dysflow`,
/// lien vers `$PREFIX/lib/dysflow/dysflow`, trouve ses voisins dans
/// `lib/dysflow/` (ONNX Runtime, `dysflow-espeak`, `espeak-ng-data`).
pub fn exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    exe.parent().map(Path::to_path_buf)
}

/// Le programme `dysflow-espeak` : `DYSFLOW_ESPEAK` (l'app y met le sien), à
/// côté du binaire, puis le `PATH`.
pub fn locate_program() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DYSFLOW_ESPEAK").map(PathBuf::from) {
        return is_executable(&path).then_some(path);
    }
    let beside = exe_dir().map(|d| d.join("dysflow-espeak"));
    if let Some(path) = beside.filter(|p| is_executable(p)) {
        return Some(path);
    }
    find_in_path(&std::env::var_os("PATH")?)
}

/// `dysflow-espeak` dans un `PATH`, dont les entrées vides ou relatives (le dossier
/// courant, donc n'importe quoi) sont ignorées.
fn find_in_path(paths: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(paths)
        .filter(|d| d.is_absolute())
        .map(|d| d.join("dysflow-espeak"))
        .find(|p| is_executable(p))
}

/// Un fichier ordinaire que l'on a le droit d'exécuter.
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Le dossier qui contient `espeak-ng-data`, quand il n'est pas celui du
/// système : `DYSFLOW_ESPEAK_DATA` (l'app : `Contents/Resources`), sinon le
/// dossier du binaire s'il le porte (paquet terminal de `package.sh`).
pub fn data_dir() -> Option<PathBuf> {
    data_dir_from(std::env::var_os("DYSFLOW_ESPEAK_DATA"), exe_dir().as_deref())
}

fn data_dir_from(variable: Option<std::ffi::OsString>, exe_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = variable {
        return Some(PathBuf::from(dir));
    }
    exe_dir.filter(|d| d.join("espeak-ng-data").is_dir()).map(Path::to_path_buf)
}

/// Un morceau de texte entre deux ponctuations, ou la ponctuation elle-même
/// avec ses espaces.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Marks(String),
}

/// Prépare le texte comme misaki : guillemets français en guillemets
/// courbes, parenthèses en chevrons (que phonemizer garde). Retours à la ligne
/// et autres caractères de contrôle deviennent des espaces : un morceau, une
/// ligne.
fn prepare(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '«' => '\u{201c}',
            '»' => '\u{201d}',
            '(' => '«',
            ')' => '»',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}


/// Un espace où l'on peut couper : pas les espaces insécables (U+00A0,
/// U+202F, U+2007), qui lient un nombre à son unité ou un mot à sa ponctuation.
fn breakable(c: char) -> bool {
    c.is_whitespace() && !matches!(c, '\u{a0}' | '\u{202f}' | '\u{2007}')
}

/// Coupe un morceau trop long pour une ligne d'espeak-ng : à un espace
/// sécable, ou, faute d'espace, à une frontière de caractère.
fn split_long(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut rest = text.trim();
    while rest.len() > MAX_LINE {
        let mut end = MAX_LINE;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let cut = match rest[..end].rfind(breakable) {
            Some(space) if !rest[..space].trim().is_empty() => space,
            _ => end,
        };
        chunks.push(rest[..cut].trim_end_matches(breakable).to_string());
        rest = rest[cut..].trim_start_matches(breakable);
    }
    if !rest.is_empty() {
        chunks.push(rest.to_string());
    }
    chunks
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

/// Un lien avec `dysflow-espeak` : ce qu'on lui écrit, les trames qu'il
/// renvoie (lues par un fil), et le processus lui-même.
struct Running {
    writer: Box<dyn Write + Send>,
    frames: Receiver<Vec<u8>>,
    child: Option<Child>,
}

/// Une trame : u32 petit-boutiste, puis les octets.
fn push_frame(request: &mut Vec<u8>, bytes: &[u8]) {
    request.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    request.extend_from_slice(bytes);
}

impl Running {
    /// Démarre le fil qui lit les trames, puis annonce la voix.
    fn start(
        writer: Box<dyn Write + Send>,
        reader: Box<dyn Read + Send>,
        child: Option<Child>,
        voice: &str,
    ) -> Result<Self> {
        let (tx, frames) = mpsc::channel();
        let thread = std::thread::Builder::new().name("dysflow-espeak".into()).spawn(move || {
            let mut reader = BufReader::new(reader);
            loop {
                let mut size = [0u8; 4];
                if reader.read_exact(&mut size).is_err() {
                    break;
                }
                let size = u32::from_le_bytes(size) as usize;
                // Une trame plus longue que tout ce qu'on a demandé : le
                // programme déraille, on cesse de le lire.
                if size > MAX_FRAME {
                    break;
                }
                let mut frame = vec![0; size];
                if reader.read_exact(&mut frame).is_err() || tx.send(frame).is_err() {
                    break;
                }
            }
        });
        let mut running = Self { writer, frames, child };
        // Désormais, `Running` tue et attend le processus s'il est relâché.
        thread.context(crate::tr!("dysflow-espeak reading thread", "fil de lecture de dysflow-espeak"))?;
        let mut request = Vec::new();
        push_frame(&mut request, voice.as_bytes());
        running.send(&request)?;
        if running.read_frame()? != b"ok" {
            bail!(crate::tr!("dysflow-espeak refuses the voice", "dysflow-espeak refuse la voix"));
        }
        Ok(running)
    }

    fn send(&mut self, request: &[u8]) -> Result<()> {
        self.writer.write_all(request).context(crate::tr!("writing to dysflow-espeak", "écriture vers dysflow-espeak"))?;
        self.writer.flush().context(crate::tr!("writing to dysflow-espeak", "écriture vers dysflow-espeak"))
    }

    fn read_frame(&self) -> Result<Vec<u8>> {
        self.frames.recv_timeout(ANSWER_TIMEOUT).map_err(|_| anyhow!(crate::tr!("dysflow-espeak no longer answers", "dysflow-espeak ne répond plus")))
    }

    /// Prêt pour une requête : vivant, et rien d'une requête passée ne reste
    /// à lire.
    fn is_clean(&mut self) -> bool {
        let alive = match &mut self.child {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => true,
        };
        alive && matches!(self.frames.try_recv(), Err(mpsc::TryRecvError::Empty))
    }

    /// Une requête, en lots d'au plus `MAX_BATCH` octets : une réponse par
    /// ligne, puis « ok ». Le découpage ne tient qu'aux longueurs.
    fn ask(&mut self, lines: &[String]) -> Result<Vec<String>> {
        if lines.iter().any(|l| l.len() > MAX_LINE) {
            bail!(crate::tr!("input too long for dysflow-espeak", "entrée trop longue pour dysflow-espeak"));
        }
        let mut answers = Vec::with_capacity(lines.len());
        let mut start = 0;
        while start < lines.len() {
            let mut size = 4;
            let mut end = start;
            while end < lines.len() && (end == start || size + 4 + lines[end].len() <= MAX_BATCH) {
                size += 4 + lines[end].len();
                end += 1;
            }
            let mut request = Vec::with_capacity(size);
            request.extend_from_slice(&((end - start) as u32).to_le_bytes());
            for line in &lines[start..end] {
                push_frame(&mut request, line.as_bytes());
            }
            self.send(&request)?;
            for _ in start..end {
                answers.push(String::from_utf8_lossy(&self.read_frame()?).into_owned());
            }
            if self.read_frame()? != b"ok" {
                bail!(crate::tr!("too many answers from dysflow-espeak", "réponses de dysflow-espeak en trop"));
            }
            start = end;
        }
        Ok(answers)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Une façon de lancer `dysflow-espeak` : le vrai programme, ou, dans les
/// tests, un faux qui tourne dans un fil.
enum Launcher {
    Program { program: PathBuf, data: Option<PathBuf> },
    #[cfg(test)]
    Fake(std::sync::Arc<dyn Fn(std::io::PipeReader, std::io::PipeWriter) + Send + Sync>),
}

/// espeak-ng, par `dysflow-espeak`, un processus par langue, lancé à la
/// demande.
pub struct EspeakProcess {
    launcher: Launcher,
    running: HashMap<&'static str, Running>,
}

impl EspeakProcess {
    pub fn new(program: PathBuf, data: Option<PathBuf>) -> Self {
        Self { launcher: Launcher::Program { program, data }, running: HashMap::new() }
    }

    /// Le programme trouvé, ou la raison de son absence.
    pub fn locate() -> Result<Self> {
        match locate_program() {
            Some(program) => Ok(Self::new(program, data_dir())),
            None => bail!(crate::tr!(
                "Incomplete Dysflow installation (dysflow-espeak not found)",
                "Installation de Dysflow incomplète (dysflow-espeak introuvable)"
            )),
        }
    }

    fn spawn(&self, voice: &str) -> Result<Running> {
        match &self.launcher {
            Launcher::Program { program, data } => {
                let mut command = Command::new(program);
                if let Some(data) = data {
                    command.arg(data);
                }
                let mut child = command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    // Doit rester `Stdio::null()` : libespeak-ng écrit une
                    // erreur par mot qu'elle ne sait pas lire ; un tube que
                    // personne ne vide finit plein, et l'assistant bloque.
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| crate::tr!(en = "starting {}", fr = "lancement de {}", program.display()))?;
                let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(crate::tr!("dysflow-espeak pipes missing", "tubes de dysflow-espeak absents"));
                };
                Running::start(Box::new(stdin), Box::new(stdout), Some(child), voice)
            }
            #[cfg(test)]
            Launcher::Fake(fake) => {
                let (from_us, to_fake) = std::io::pipe()?;
                let (from_fake, to_us) = std::io::pipe()?;
                let fake = fake.clone();
                std::thread::spawn(move || fake(from_us, to_us));
                Running::start(Box::new(to_fake), Box::new(from_fake), None, voice)
            }
        }
    }

    fn ask(&mut self, voice: &'static str, lines: &[String]) -> Result<Vec<String>> {
        // Un processus mort, ou dont une réponse traîne encore, est remplacé.
        if self.running.get_mut(voice).is_some_and(|r| !r.is_clean()) {
            self.running.remove(voice);
        }
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
        // Chaque morceau de texte, en une ou plusieurs lignes.
        let chunked: Vec<Vec<String>> = pieces
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Piece::Text(t) => Some(split_long(t)),
                Piece::Marks(_) => None,
            })
            .collect();
        let lines: Vec<String> = chunked.iter().flatten().cloned().collect();
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
        if answers.len() != lines.len() {
            self.running.remove(voice);
            bail!(crate::tr!("incomplete answers from dysflow-espeak", "réponses de dysflow-espeak incomplètes"));
        }
        let mut answers = answers.into_iter();
        let mut joined = chunked.iter().map(|chunks| {
            let parts: Vec<String> = answers.by_ref().take(chunks.len()).collect();
            parts.join(" ")
        });
        Ok(pieces
            .iter()
            .map(|item| {
                let mut raw = String::new();
                for piece in item {
                    match piece {
                        Piece::Text(_) => raw.push_str(&joined.next().unwrap_or_default()),
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
    use std::io::{PipeReader, PipeWriter};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn the_espeak_data_shipped_beside_the_binary_is_found() {
        let dir = crate::paths::test_scratch("espeak-data");
        std::fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        // La variable d'abord ; sinon le dossier du binaire, s'il porte les
        // données (paquet terminal) ; sinon celles du système.
        assert_eq!(data_dir_from(Some("/ailleurs".into()), Some(&dir)), Some(PathBuf::from("/ailleurs")));
        assert_eq!(data_dir_from(None, Some(&dir)), Some(dir.clone()));
        assert_eq!(data_dir_from(None, Some(Path::new("/nulle/part"))), None);
        assert_eq!(data_dir_from(None, None), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn read_u32(r: &mut PipeReader) -> Option<u32> {
        let mut b = [0u8; 4];
        r.read_exact(&mut b).ok()?;
        Some(u32::from_le_bytes(b))
    }

    fn read_frame(r: &mut PipeReader) -> Option<String> {
        let size = read_u32(r)? as usize;
        let mut bytes = vec![0; size];
        r.read_exact(&mut bytes).ok()?;
        Some(String::from_utf8(bytes).unwrap())
    }

    /// Des trames mises bout à bout, chacune précédée de sa longueur.
    fn framed(frames: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for frame in frames {
            push_frame(&mut bytes, frame);
        }
        bytes
    }

    fn write_raw(w: &mut PipeWriter, bytes: &[u8]) -> std::io::Result<()> {
        w.write_all(bytes)?;
        w.flush()
    }

    /// Ce que le faux répond à un lot : les octets à écrire (en général les
    /// trames des réponses puis « ok »), ou `None` pour mourir.
    type Answer = dyn Fn(usize, &[String]) -> Option<Vec<u8>> + Send + Sync;

    /// Un faux `dysflow-espeak` dans un fil, qui parle le vrai protocole.
    /// `answer` reçoit le numéro du lot (compté sur toutes les relances) et
    /// ses entrées.
    fn fake(answer: impl Fn(usize, &[String]) -> Option<Vec<u8>> + Send + Sync + 'static) -> EspeakProcess {
        let answer: Arc<Answer> = Arc::new(answer);
        let batches = Arc::new(AtomicUsize::new(0));
        let run = move |mut r: PipeReader, mut w: PipeWriter| {
            if read_frame(&mut r).is_none() || write_raw(&mut w, &framed(&[b"ok".to_vec()])).is_err() {
                return;
            }
            while let Some(count) = read_u32(&mut r) {
                let Some(items) = (0..count).map(|_| read_frame(&mut r)).collect::<Option<Vec<_>>>() else {
                    return;
                };
                let Some(frames) = answer(batches.fetch_add(1, Ordering::SeqCst), &items) else {
                    return;
                };
                if write_raw(&mut w, &frames).is_err() {
                    return;
                }
            }
        };
        EspeakProcess { launcher: Launcher::Fake(Arc::new(run)), running: HashMap::new() }
    }

    fn echo_frames(items: &[String]) -> Vec<Vec<u8>> {
        items.iter().map(|i| format!("<{i}>").into_bytes()).chain([b"ok".to_vec()]).collect()
    }

    fn echo(items: &[String]) -> Vec<u8> {
        framed(&echo_frames(items))
    }

    fn echoing() -> EspeakProcess {
        fake(|_, items| Some(echo(items)))
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn each_item_gets_its_own_answer_whatever_it_says() {
        let mut espeak = echoing();
        let out = espeak.phonemize("fr", &strings(&["bonjour", "le monde"])).unwrap();
        assert_eq!(out, ["<bonjour>", "<le monde>"]);
        // Ce qu'un texte contient ne touche pas au découpage.
        let tricky = strings(&["ok", "Zyzomys Zyzemys", "a\u{3002}b\u{2029}c", "[[h@loU]]", "x \u{2013} y"]);
        let out = espeak.phonemize("fr", &tricky).unwrap();
        // (`finish` réduit les espaces, U+2029 compris.)
        assert_eq!(out, ["<ok>", "<Zyzomys Zyzemys>", "<a\u{3002}b c>", "<[[h@loU]]>", "<x \u{2013} y>"]);
        // Un second aller-retour sur le même processus.
        assert_eq!(espeak.phonemize("fr", &strings(&["encore"])).unwrap(), ["<encore>"]);
    }

    #[test]
    fn punctuation_stays_where_it_was() {
        let mut espeak = echoing();
        let out = espeak.phonemize("fr", &strings(&["Peut-être : les notes, et (l'ordre)."])).unwrap();
        // Les morceaux entre ponctuations partent seuls ; « - » disparaît
        // comme chez misaki, et les parenthèses reviennent.
        assert_eq!(out, ["<Peutêtre> : <les notes>, <et> (<l'ordre>)."]);
    }

    #[test]
    fn an_empty_item_gives_an_empty_answer() {
        let mut espeak = echoing();
        assert_eq!(espeak.phonemize("fr", &strings(&["", "."])).unwrap(), ["", "."]);
    }

    #[test]
    fn too_many_answers_is_an_error_and_the_next_call_starts_afresh() {
        // Les deux premiers lots (l'essai et sa relance) reçoivent une réponse
        // de trop.
        let mut espeak = fake(|batch, items| {
            let mut frames = echo_frames(items);
            if batch < 2 {
                frames.insert(0, b"<en trop>".to_vec());
            }
            Some(framed(&frames))
        });
        assert!(espeak.phonemize("fr", &strings(&["a", "b"])).is_err());
        assert_eq!(espeak.phonemize("fr", &strings(&["a", "b"])).unwrap(), ["<a>", "<b>"]);
    }

    #[test]
    fn too_few_answers_is_an_error_and_the_next_call_starts_afresh() {
        let mut espeak = fake(|batch, items| {
            let mut frames = echo_frames(items);
            if batch < 2 {
                frames.remove(0);
            }
            Some(framed(&frames))
        });
        assert!(espeak.phonemize("fr", &strings(&["a", "b"])).is_err());
        assert_eq!(espeak.phonemize("fr", &strings(&["a", "b"])).unwrap(), ["<a>", "<b>"]);
    }

    #[test]
    fn an_oversized_answer_is_an_error_and_the_next_call_starts_afresh() {
        let mut espeak = fake(|batch, items| {
            if batch < 2 {
                // Une longueur annoncée démesurée.
                return Some(u32::MAX.to_le_bytes().to_vec());
            }
            Some(echo(items))
        });
        let t = std::time::Instant::now();
        assert!(espeak.phonemize("fr", &strings(&["a"])).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "sans attendre : {:?}", t.elapsed());
        assert_eq!(espeak.phonemize("fr", &strings(&["a"])).unwrap(), ["<a>"]);
    }

    #[test]
    fn an_oversized_item_is_refused_before_sending() {
        let mut espeak = echoing();
        assert!(espeak.ask("fr-fr", &["a".repeat(MAX_LINE + 1)]).is_err());
        assert_eq!(espeak.ask("fr-fr", &["a".repeat(MAX_LINE)]).unwrap(), [format!("<{}>", "a".repeat(MAX_LINE))]);
    }

    #[test]
    fn a_dead_helper_is_restarted_once() {
        let mut espeak = fake(|batch, items| (batch != 0).then(|| echo(items)));
        assert_eq!(espeak.phonemize("fr", &strings(&["meurs"])).unwrap(), ["<meurs>"]);
        let mut doomed = fake(|batch, items| (batch >= 2).then(|| echo(items)));
        assert!(doomed.phonemize("fr", &strings(&["meurs"])).is_err(), "deux morts : échec");
        assert_eq!(doomed.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant>"], "on repart");
    }

    #[test]
    fn a_silent_helper_fails_quickly_and_the_next_call_works() {
        let mut espeak = fake(|batch, items| Some(if batch < 2 { Vec::new() } else { echo(items) }));
        let t = std::time::Instant::now();
        assert!(espeak.phonemize("fr", &strings(&["tais-toi"])).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(6), "{:?}", t.elapsed());
        assert_eq!(espeak.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant>"]);
    }

    /// Un vrai processus, écrit en shell.
    fn script(name: &str, body: &str) -> EspeakProcess {
        let dir = crate::paths::test_scratch(&format!("espeak-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dysflow-espeak");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        EspeakProcess::new(path, None)
    }

    #[test]
    fn a_helper_process_that_exits_or_stays_mute_is_an_error() {
        let t = std::time::Instant::now();
        assert!(script("sortie", "exit 1").phonemize("fr", &strings(&["a"])).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "{:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert!(script("muet", "exec cat > /dev/null").phonemize("fr", &strings(&["a"])).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(6), "{:?}", t.elapsed());
    }

    #[test]
    fn a_request_larger_than_a_pipe_goes_in_batches() {
        let mut espeak = echoing();
        let items: Vec<String> = (0..2000).map(|i| format!("mot numéro {i} assez long pour remplir")).collect();
        let out = espeak.phonemize("fr", &items).unwrap();
        assert_eq!(out.len(), 2000);
        assert!(out.iter().zip(&items).all(|(o, i)| o == &format!("<{i}>")));
    }

    #[test]
    fn a_helper_that_stops_reading_never_blocks_a_large_request() {
        // Après l'annonce de la voix, le faux ne lit plus rien : 2 000 lignes
        // écrites d'un bloc attendraient indéfiniment.
        let run = |mut r: PipeReader, mut w: PipeWriter| {
            if read_frame(&mut r).is_some() && write_raw(&mut w, &framed(&[b"ok".to_vec()])).is_ok() {
                std::thread::sleep(std::time::Duration::from_secs(20));
            }
        };
        let mut espeak = EspeakProcess { launcher: Launcher::Fake(Arc::new(run)), running: HashMap::new() };
        let items: Vec<String> = (0..2000).map(|i| format!("mot numéro {i} assez long pour remplir")).collect();
        let t = std::time::Instant::now();
        assert!(espeak.phonemize("fr", &items).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(6), "{:?}", t.elapsed());
    }

    /// Les morceaux `<…>` renvoyés par le faux, dans l'ordre.
    fn received(out: &str) -> Vec<String> {
        out.strip_prefix('<').unwrap().strip_suffix('>').unwrap().split("> <").map(str::to_string).collect()
    }

    #[test]
    fn a_long_piece_is_sent_in_bounded_lines() {
        let mut espeak = echoing();
        let words = "été ".repeat(300);
        let words = words.trim();
        let glued = "é".repeat(700);
        let out = espeak.phonemize("fr", &[words.to_string(), glued.clone()]).unwrap();
        let first = received(&out[0]);
        assert!(first.len() >= 2, "{} lignes", first.len());
        assert!(first.iter().all(|l| l.len() <= MAX_LINE && !l.starts_with(' ') && !l.ends_with(' ')));
        assert!(first.iter().all(|l| l.split(' ').all(|w| w == "été")), "jamais en plein mot");
        assert_eq!(first.join(" "), words);
        let second = received(&out[1]);
        assert!(second.len() >= 2 && second.iter().all(|l| l.len() <= MAX_LINE));
        assert_eq!(second.concat(), glued, "coupé entre deux caractères");
    }

    #[test]
    fn control_characters_become_spaces() {
        let mut espeak = echoing();
        let out = espeak.phonemize("fr", &strings(&["un\u{7}deux\u{b}trois\nquatre\u{0}cinq"])).unwrap();
        assert_eq!(out, ["<un deux trois quatre cinq>"]);
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

    #[test]
    fn the_path_search_skips_relative_entries_and_plain_files() {
        let root = crate::paths::test_scratch("espeak-path");
        let (plain, exec) = (root.join("plain"), root.join("exec"));
        for d in [&plain, &exec] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("dysflow-espeak"), "#!/bin/sh\n").unwrap();
        }
        std::fs::set_permissions(plain.join("dysflow-espeak"), std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(exec.join("dysflow-espeak"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = |dirs: &[&std::path::Path]| std::env::join_paths(dirs).unwrap();
        assert_eq!(find_in_path(&path(&[&plain])), None, "pas exécutable");
        assert_eq!(find_in_path(&path(&[&plain, &exec])), Some(exec.join("dysflow-espeak")));
        // Un dossier relatif (ici, le même dossier, atteint depuis le dossier
        // courant) ou une entrée vide ne comptent pas.
        let depth = std::env::current_dir().unwrap().components().count() - 1;
        let relative = std::path::PathBuf::from("../".repeat(depth)).join(exec.strip_prefix("/").unwrap());
        assert!(relative.join("dysflow-espeak").is_file(), "le chemin relatif mène bien au programme");
        assert_eq!(find_in_path(&path(&[&relative, std::path::Path::new("")])), None);
    }

    #[test]
    fn a_long_piece_is_never_cut_at_a_non_breaking_space() {
        let text = format!("{} fin", "10\u{a0}km\u{202f}!".repeat(150));
        let chunks = split_long(&text);
        assert!(chunks.len() >= 2 && chunks.iter().all(|c| c.len() <= MAX_LINE));
        assert_eq!(chunks.concat().replace(' ', ""), text.replace(' ', ""), "aucun insécable perdu ni coupé");
        let normal = split_long(&("mot\u{a0}lié ".repeat(150)));
        assert!(normal.len() >= 2 && normal.iter().all(|c| c.split(' ').all(|w| w == "mot\u{a0}lié")));
    }

    /// Parité avec misaki : `DYSFLOW_ESPEAK=target/espeak/bin/espeak-ng
    /// DYSFLOW_ESPEAK_DATA=target/espeak/share cargo test --lib kokoro::phonemes -- --ignored`
    #[test]
    #[ignore]
    fn french_phonemes_match_misaki() {
        let reference: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
            serde_json::from_str(include_str!("../../../tests/fixtures/kokoro/misaki-phonemes.json")).unwrap();
        let mut espeak = EspeakProcess::locate().expect("dysflow-espeak construit");
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
        let mut espeak = EspeakProcess::locate().expect("dysflow-espeak construit");
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
        let mut espeak = EspeakProcess::locate().expect("dysflow-espeak construit");
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

    /// Le vrai programme : des textes qui faisaient écrire plusieurs lignes à
    /// espeak-ng, ou qui imitaient ses marques, reçoivent chacun leurs propres
    /// phonèmes, jamais ceux d'un autre. `DYSFLOW_ESPEAK=target/espeak/bin/dysflow-espeak
    /// DYSFLOW_ESPEAK_DATA=target/espeak/share cargo test --lib kokoro::phonemes -- --ignored`
    #[test]
    #[ignore]
    fn the_real_helper_gives_each_item_its_own_phonemes() {
        let mut espeak = EspeakProcess::locate().expect("dysflow-espeak construit");
        let cases: [(&str, Vec<String>); 2] = [
            (
                "en",
                strings(&[
                    "Zyzemys\u{3002}hello\u{3002}Zyzemys\u{3002}Xq\u{3002}Qx\u{3002}Zv",
                    "the bank account number",
                    "a \u{2013} b\u{2029}c\u{37e} d\u{ff01} e",
                    "[[h@l'oU]] world",
                    "Zyzomys Zyzemys Xq",
                ]),
            ),
            ("fr", strings(&["Zyzomys\u{3002}bonjour", "le monde", "[[bOZur]] encore", "x \u{2013} y"])),
        ];
        for (language, items) in cases {
            let alone: Vec<String> =
                items.iter().map(|i| espeak.phonemize(language, std::slice::from_ref(i)).unwrap().remove(0)).collect();
            assert!(alone.iter().all(|a| !a.is_empty()));
            // Les phonèmes entre [[ ]] ne passent pas tels quels.
            assert!(alone.iter().all(|a| !a.contains("h@l") && !a.contains("bOZ")));
            for _ in 0..1000 {
                assert_eq!(espeak.phonemize(language, &items).unwrap(), alone);
            }
        }
    }

    /// Le vrai programme refuse une entrée trop longue en s'arrêtant.
    #[test]
    #[ignore]
    fn the_real_helper_stops_on_an_oversized_item() {
        let program = locate_program().expect("dysflow-espeak construit");
        let mut command = Command::new(program);
        if let Some(data) = data_dir() {
            command.arg(data);
        }
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let mut request = Vec::new();
        push_frame(&mut request, b"fr-fr");
        request.extend_from_slice(&1u32.to_le_bytes());
        request.extend_from_slice(&(70_000u32).to_le_bytes());
        child.stdin.take().unwrap().write_all(&request).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success(), "{status:?}");
    }
}

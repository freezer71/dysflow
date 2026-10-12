//! Phonétisation par espeak-ng, lancé comme programme séparé : la GPL-3.0
//! d'espeak-ng reste de son côté. Le traitement de misaki (`EspeakG2P` pour
//! le français, `EspeakFallback` pour l'anglais) est porté tel quel : Kokoro
//! a été entraîné sur ces phonèmes.
//!
//! Protocole : une ligne de texte, puis une ligne sentinelle ; tout ce que
//! espeak-ng écrit avant la réponse connue de la sentinelle appartient à la
//! ligne. Un texte pourrait répondre comme la sentinelle : la sentinelle ne
//! fait donc que séparer, et c'est la clôture qui valide le lot. Chaque lot se
//! termine par trois mots de clôture tirés au hasard (parmi huit dont les
//! réponses sont apprises au lancement) ; le lot n'est accepté que si leurs
//! réponses arrivent dans cet ordre, précédées d'exactement une sentinelle par
//! ligne envoyée. Sinon le processus est relancé et la requête échoue : jamais
//! de réponses décalées. Aucun texte n'est jamais journalisé.

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
/// Les mots de clôture, courts (peu coûteux) et aux réponses distinctes en
/// français comme en anglais. Un texte peut en contenir : il ne peut pas
/// deviner la suite tirée pour le lot.
const CLOSINGS: [&str; 8] = ["Xq.", "Qx.", "Zv.", "Vz.", "Kx.", "Jq.", "Wz.", "Qj."];
/// La longueur de la suite de clôture d'un lot.
const CLOSING_RUN: usize = 3;
/// Au-delà de tant de lignes par ligne envoyée, espeak-ng s'emballe.
const MAX_LINES_PER_LINE: usize = 64;
/// espeak-ng lit son entrée par `fgets` de 1 000 octets : une ligne plus
/// longue serait coupée en plein mot, voire en plein caractère.
const MAX_LINE: usize = 900;
/// Un lot tient dans le tampon d'un tube vide (16 Kio sous macOS) : l'écrire
/// ne bloque jamais, même si espeak-ng ne lit plus.
const MAX_BATCH: usize = 8 * 1024;
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
        return is_executable(&path).then_some(path);
    }
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("espeak-ng")));
    if let Some(path) = beside.filter(|p| is_executable(p)) {
        return Some(path);
    }
    find_in_path(&std::env::var_os("PATH")?)
}

/// espeak-ng dans un `PATH`, dont les entrées vides ou relatives (le dossier
/// courant, donc n'importe quoi) sont ignorées.
fn find_in_path(paths: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(paths)
        .filter(|d| d.is_absolute())
        .map(|d| d.join("espeak-ng"))
        .find(|p| is_executable(p))
}

/// Un fichier ordinaire que l'on a le droit d'exécuter.
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
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

/// Éloigne de la sentinelle les mots qui commencent comme elle (« zyzom… »,
/// quelles que soient casse et accents : « Zyzomys », « Zyzomies ») : leurs
/// « y » deviennent des « i ». Ce n'est qu'une précaution, pour qu'un tel
/// texte se lise au lieu d'échouer : la clôture du lot détecte de toute façon
/// une réponse qui imiterait la sentinelle.
fn neutralize(text: &str) -> String {
    const WORD: [char; 5] = ['z', 'y', 'z', 'o', 'm'];
    // Ce qu'espeak-ng ne prononce pas entre deux lettres : diacritiques
    // combinants, espaces sans chasse, trait d'union conditionnel.
    fn silent(c: char) -> bool {
        matches!(c, '\u{300}'..='\u{36f}' | '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' | '\u{ad}')
    }
    fn base(c: char) -> char {
        match c.to_lowercase().next().unwrap_or(c) {
            'ý' | 'ÿ' | 'ŷ' | 'ỳ' => 'y',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' => 'o',
            'ž' | 'ź' | 'ż' => 'z',
            'š' | 'ś' | 'ş' => 's',
            other => other,
        }
    }
    let mut chars: Vec<char> = text.chars().collect();
    let letters: Vec<usize> = (0..chars.len()).filter(|&i| !silent(chars[i])).collect();
    for start in 0..letters.len().saturating_sub(WORD.len() - 1) {
        let window = &letters[start..start + WORD.len()];
        if window.iter().zip(WORD).all(|(&i, w)| base(chars[i]) == w) {
            // Le « y » du préfixe, puis ceux du reste du mot.
            let rest: Vec<usize> =
                letters[start + WORD.len()..].iter().copied().take_while(|&i| chars[i].is_alphabetic()).collect();
            for i in std::iter::once(window[1]).chain(rest) {
                if base(chars[i]) == 'y' {
                    chars[i] = if chars[i].is_uppercase() { 'I' } else { 'i' };
                }
            }
        }
    }
    chars.into_iter().collect()
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

/// Une suite de clôture tirée au hasard : `RandomState` est semé par le
/// système à chaque fil et change à chaque appel, SipHash le rend imprévisible.
fn closing_run() -> [usize; CLOSING_RUN] {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    let mut bits = hasher.finish();
    let mut pool: Vec<usize> = (0..CLOSINGS.len()).collect();
    let mut run = [0; CLOSING_RUN];
    for slot in &mut run {
        let pick = (bits % pool.len() as u64) as usize;
        bits /= pool.len() as u64;
        *slot = pool.swap_remove(pick);
    }
    run
}

struct Running {
    process: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    sentinel: String,
    /// Les lignes de réponse de chaque mot de `CLOSINGS`.
    closings: Vec<Vec<String>>,
}

impl Running {
    fn read_line(&self) -> Result<String> {
        self.lines.recv_timeout(ANSWER_TIMEOUT).map_err(|_| anyhow!("espeak-ng ne répond plus"))
    }

    /// Les lignes qui précèdent la prochaine réponse de la sentinelle.
    fn read_answer(&self) -> Result<Vec<String>> {
        let mut parts = Vec::new();
        loop {
            let line = self.read_line()?;
            if line.trim() == self.sentinel {
                return Ok(parts);
            }
            parts.push(line.trim().to_string());
        }
    }

    /// Apprend les réponses des mots de clôture, deux fois : elles doivent
    /// être stables, non vides et toutes différentes.
    fn learn(&mut self) -> Result<()> {
        let mut request = format!("{SENTINEL}\n");
        for _ in 0..2 {
            for word in CLOSINGS {
                request.push_str(&format!("{word}\n{SENTINEL}\n"));
            }
        }
        self.stdin.write_all(request.as_bytes())?;
        self.stdin.flush()?;
        self.sentinel = self.read_line()?.trim().to_string();
        let mut rounds = Vec::new();
        for _ in 0..2 {
            let round: Vec<Vec<String>> = CLOSINGS.iter().map(|_| self.read_answer()).collect::<Result<_>>()?;
            rounds.push(round);
        }
        let closings = rounds.pop().expect("deux tours");
        let distinct: std::collections::HashSet<&Vec<String>> = closings.iter().collect();
        if self.sentinel.is_empty()
            || closings != rounds[0]
            || distinct.len() != CLOSINGS.len()
            || closings.iter().any(|c| c.is_empty() || c.iter().any(String::is_empty))
        {
            bail!("espeak-ng ne répond pas comme prévu");
        }
        self.closings = closings;
        Ok(())
    }

    /// Prêt pour une requête : vivant, et rien d'une requête passée ne reste
    /// à lire (sinon les réponses seraient décalées).
    fn is_clean(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
            && matches!(self.lines.try_recv(), Err(mpsc::TryRecvError::Empty))
    }

    /// Une requête, en lots : chaque ligne suivie de la sentinelle, la
    /// clôture à la fin de chaque lot, une réponse par ligne.
    fn ask(&mut self, lines: &[String]) -> Result<Vec<String>> {
        let mut answers = Vec::with_capacity(lines.len());
        let mut start = 0;
        while start < lines.len() {
            let mut request = String::new();
            let mut end = start;
            while end < lines.len()
                && (end == start || request.len() + lines[end].len() + SENTINEL.len() + 2 <= MAX_BATCH)
            {
                request.push_str(&lines[end]);
                request.push('\n');
                request.push_str(SENTINEL);
                request.push('\n');
                end += 1;
            }
            let run = closing_run();
            let mut expected = Vec::new();
            for &word in &run {
                request.push_str(CLOSINGS[word]);
                request.push('\n');
                expected.extend(self.closings[word].iter().cloned());
            }
            self.stdin.write_all(request.as_bytes()).context("écriture vers espeak-ng")?;
            self.stdin.flush().context("écriture vers espeak-ng")?;
            // Tout jusqu'à la suite de clôture, puis : une sentinelle après
            // chaque ligne, ni plus ni moins.
            let count = end - start;
            let limit = MAX_LINES_PER_LINE * (count + 1) + expected.len();
            let mut received: Vec<String> = Vec::new();
            while !received.ends_with(&expected) {
                if received.len() >= limit {
                    bail!("espeak-ng répond trop");
                }
                received.push(self.read_line()?.trim().to_string());
            }
            received.truncate(received.len() - expected.len());
            if received.iter().filter(|l| **l == self.sentinel).count() != count
                || received.last() != Some(&self.sentinel)
            {
                bail!("réponses d'espeak-ng décalées");
            }
            received.pop();
            for answer in received.split(|l| *l == self.sentinel) {
                answers.push(answer.join(" "));
            }
            start = end;
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
        let (Some(stdin), Some(stdout)) = (process.stdin.take(), process.stdout.take()) else {
            let _ = process.kill();
            let _ = process.wait();
            bail!("tubes d'espeak-ng absents");
        };
        let (tx, lines) = mpsc::channel();
        let reader = std::thread::Builder::new().name("espeak-ng".into()).spawn(move || {
            let mut stdout = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match stdout.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string();
                if tx.send(text).is_err() {
                    break;
                }
            }
        });
        if let Err(e) = reader {
            let _ = process.kill();
            let _ = process.wait();
            return Err(e).context("fil de lecture d'espeak-ng");
        }
        // Désormais, `Running` tue et attend le processus s'il est relâché.
        let mut running = Running { process, stdin, lines, sentinel: String::new(), closings: Vec::new() };
        // Les réponses de la sentinelle et des clôtures, apprises une fois.
        running.learn()?;
        Ok(running)
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
                Piece::Text(t) => Some(split_long(&neutralize(t))),
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
            bail!("réponses d'espeak-ng incomplètes");
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

    /// Un faux espeak-ng dont `special` (du shell, placé avant la réponse
    /// ordinaire `<texte>` puis `x`) traite certaines lignes à part.
    fn fake_with(name: &str, special: &str) -> EspeakProcess {
        let dir = std::env::temp_dir().join(format!("sillon-espeak-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("espeak-ng");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nwhile IFS= read -r line; do\n  if [ \"$line\" = \"{SENTINEL}\" ]; then echo 'zizomis'; continue; fi\n{special}\n  echo \"<$line>\"; echo x\ndone\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        EspeakProcess::new(script, None)
    }

    /// Comme le vrai espeak-ng : le mot de la sentinelle, quelle que soit sa
    /// casse, répond exactement comme elle.
    const LIKE_ESPEAK: &str = "  case \"$line\" in [Zz][Yy][Zz][Oo][Mm][Yy][Ss]|Zýzomys) echo 'zizomis'; continue;; esac";

    #[test]
    fn the_sentinel_word_in_a_text_cannot_end_an_answer_early() {
        let mut espeak = fake_with("mot-sentinelle", LIKE_ESPEAK);
        let out = espeak.phonemize("fr", &strings(&["Zyzomys", "ZYZOMYS", "Zýzomys", "après"])).unwrap();
        assert_eq!(out, ["<Zizomis> x", "<ZIZOMIS> x", "<Zizomis> x", "<après> x"]);
        assert_eq!(espeak.phonemize("fr", &strings(&["encore"])).unwrap(), ["<encore> x"]);
    }

    #[test]
    fn neutralizing_ignores_case_accents_and_silent_marks() {
        assert_eq!(neutralize("le zyzomys, ZyZoMyS !"), "le zizomis, ZiZoMiS !");
        assert_eq!(neutralize("Zy\u{301}zo\u{ad}mys"), "Zi\u{301}zo\u{ad}mis");
        assert_eq!(neutralize("Zyzomies, zyzomy"), "Zizomies, zizomi");
        assert_eq!(neutralize("zyzo, yzomys"), "zyzo, yzomys");
    }

    #[test]
    fn a_collision_never_shifts_the_following_answers() {
        // Une ligne inconnue qui répondrait comme la sentinelle : la requête
        // échoue, mais les suivantes ne sont pas décalées.
        let mut espeak = fake_with("collision", "  if [ \"$line\" = collision ]; then echo 'zizomis'; continue; fi");
        assert!(espeak.phonemize("fr", &strings(&["collision", "b"])).is_err());
        assert_eq!(espeak.phonemize("fr", &strings(&["vivant", "aussi"])).unwrap(), ["<vivant> x", "<aussi> x"]);
        assert!(espeak.phonemize("fr", &strings(&["collision"])).is_err(), "seule aussi");
        assert_eq!(espeak.phonemize("fr", &strings(&["encore"])).unwrap(), ["<encore> x"]);
    }

    /// Du shell : chaque mot de clôture, quelle que soit sa casse et avec ou
    /// sans point, répond comme le mot lui-même (comme le vrai espeak-ng).
    fn closings_like_espeak() -> String {
        CLOSINGS
            .iter()
            .map(|w| {
                let letters: String = w.trim_end_matches('.').chars().map(|c| format!("[{}{}]", c.to_ascii_uppercase(), c.to_ascii_lowercase())).collect();
                format!("  case \"$line\" in {letters}|{letters}.) echo '<{w}>'; echo x; continue;; esac\n")
            })
            .collect()
    }

    #[test]
    fn closing_words_in_a_text_never_shift_the_answers() {
        let special = closings_like_espeak() + "  if [ \"$line\" = imite ]; then echo 'zizomis'; continue; fi";
        let mut espeak = fake_with("clotures", &special);
        // Chaque mot de clôture, seul ou en suite, dans une entrée ou dans
        // plusieurs : les réponses sont justes.
        let mut items: Vec<String> = CLOSINGS.iter().map(|w| w.to_string()).collect();
        items.extend(CLOSINGS.iter().map(|w| w.trim_end_matches('.').to_uppercase()));
        items.push(CLOSINGS.join(" "));
        // Le point du mot seul part avec la ponctuation, comme partout.
        let mut expected: Vec<String> = CLOSINGS.iter().map(|w| format!("<{w}> x.")).collect();
        expected.extend(CLOSINGS.iter().map(|w| format!("<{w}> x")));
        expected.push(CLOSINGS.iter().map(|w| format!("<{w}> x.")).collect::<Vec<_>>().join(" "));
        for _ in 0..20 {
            assert_eq!(espeak.phonemize("fr", &items).unwrap(), expected);
        }
        // Une réponse qui imite la sentinelle : une erreur, jamais un décalage.
        for at in 0..3 {
            let mut items = strings(&["Xq", "a", "Qx."]);
            items.insert(at, "imite".into());
            assert!(espeak.phonemize("fr", &items).is_err(), "imitation en position {at}");
            assert_eq!(espeak.phonemize("fr", &strings(&["Xq", "b"])).unwrap(), ["<Xq.> x", "<b> x"]);
        }
    }

    #[test]
    fn a_text_that_replays_every_closing_run_only_gets_an_error() {
        // Une ligne dont la réponse contient toutes les suites de clôture
        // possibles : le lot s'arrête trop tôt, le compte des sentinelles le
        // voit.
        let mut replay = String::new();
        for a in CLOSINGS {
            for b in CLOSINGS {
                for c in CLOSINGS {
                    if a != b && b != c && a != c {
                        replay.push_str(&format!("echo '<{a}>'; echo x; echo '<{b}>'; echo x; echo '<{c}>'; echo x; "));
                    }
                }
            }
        }
        let special = format!("  if [ \"$line\" = rejoue ]; then {replay}continue; fi");
        let mut espeak = fake_with("rejoue", &special);
        for items in [strings(&["rejoue"]), strings(&["a", "rejoue"]), strings(&["rejoue", "b"])] {
            assert!(espeak.phonemize("fr", &items).is_err());
            assert_eq!(espeak.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant> x"]);
        }
    }

    #[test]
    fn each_batch_draws_its_own_closing_run() {
        let runs: std::collections::HashSet<[usize; CLOSING_RUN]> = (0..50).map(|_| closing_run()).collect();
        assert!(runs.len() > 10, "{} suites différentes sur 50", runs.len());
        for run in runs {
            assert!(run.iter().all(|&w| w < CLOSINGS.len()));
            assert!(run[0] != run[1] && run[1] != run[2] && run[0] != run[2]);
        }
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

    /// Les lignes `<…>` reçues par le faux, dans l'ordre.
    fn received(out: &str) -> Vec<String> {
        out.split(" x").filter_map(|p| p.trim().strip_prefix('<')?.strip_suffix('>').map(str::to_string)).collect()
    }

    #[test]
    fn a_long_piece_is_sent_in_lines_espeak_can_read() {
        let mut espeak = fake_with("long", "");
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
    fn control_characters_never_split_a_piece() {
        let mut espeak = fake_with("controle", "");
        let out = espeak.phonemize("fr", &strings(&["un\u{7}deux\u{b}trois\nquatre\u{0}cinq"])).unwrap();
        assert_eq!(out, ["<un deux trois quatre cinq> x"]);
    }

    #[test]
    fn a_silent_process_fails_quickly_and_the_next_call_works() {
        let mut espeak = fake_with("muet", "  if [ \"$line\" = tais-toi ]; then while read -r _; do :; done; fi");
        let t = std::time::Instant::now();
        assert!(espeak.phonemize("fr", &strings(&["tais-toi"])).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(6), "{:?}", t.elapsed());
        assert_eq!(espeak.phonemize("fr", &strings(&["vivant"])).unwrap(), ["<vivant> x"]);
    }

    #[test]
    fn a_request_larger_than_a_pipe_goes_in_batches() {
        let mut espeak = fake_with("lots", "");
        let items: Vec<String> = (0..2000).map(|i| format!("mot numéro {i} assez long pour remplir")).collect();
        let out = espeak.phonemize("fr", &items).unwrap();
        assert_eq!(out.len(), 2000);
        assert!(out.iter().zip(&items).all(|(o, i)| o == &format!("<{i}> x")));
    }

    #[test]
    fn a_process_that_stops_reading_never_blocks_a_large_request() {
        // 2 000 lignes, bien plus qu'un tube n'en tient : écrites d'un bloc,
        // l'écriture attendrait espeak-ng indéfiniment.
        let mut espeak = fake_with("bloque", "  if [ \"$line\" = dors ]; then exec sleep 20; fi");
        let mut items = vec!["dors".to_string()];
        items.extend((0..2000).map(|i| format!("mot numéro {i} assez long pour remplir")));
        let t = std::time::Instant::now();
        assert!(espeak.phonemize("fr", &items).is_err());
        assert!(t.elapsed() < std::time::Duration::from_secs(6), "{:?}", t.elapsed());
    }

    #[test]
    fn the_path_search_skips_relative_entries_and_plain_files() {
        let root = std::env::temp_dir().join(format!("sillon-espeak-{}-path", std::process::id()));
        let (plain, exec) = (root.join("plain"), root.join("exec"));
        for d in [&plain, &exec] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("espeak-ng"), "#!/bin/sh\n").unwrap();
        }
        std::fs::set_permissions(plain.join("espeak-ng"), std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(exec.join("espeak-ng"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = |dirs: &[&std::path::Path]| std::env::join_paths(dirs).unwrap();
        assert_eq!(find_in_path(&path(&[&plain])), None, "pas exécutable");
        assert_eq!(find_in_path(&path(&[&plain, &exec])), Some(exec.join("espeak-ng")));
        // Un dossier relatif (ici, le même dossier, atteint depuis le dossier
        // courant) ou une entrée vide ne comptent pas.
        let depth = std::env::current_dir().unwrap().components().count() - 1;
        let relative = std::path::PathBuf::from("../".repeat(depth)).join(exec.strip_prefix("/").unwrap());
        assert!(relative.join("espeak-ng").is_file(), "le chemin relatif mène bien au programme");
        assert_eq!(find_in_path(&path(&[&relative, std::path::Path::new("")])), None);
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

    /// Le vrai espeak-ng : le mot de la sentinelle et une longue ligne ne
    /// décalent ni ne corrompent rien.
    #[test]
    #[ignore]
    fn the_real_espeak_survives_the_sentinel_word_and_long_lines() {
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let reference = espeak.phonemize("fr", &strings(&["bonjour", "maison"])).unwrap();
        let out = espeak.phonemize("fr", &strings(&["Zyzomys", "ZYZOMYS", "bonjour"])).unwrap();
        assert!(!out[0].is_empty() && !out[1].is_empty());
        assert_eq!(out[2], reference[0]);
        assert_eq!(espeak.phonemize("fr", &strings(&["maison"])).unwrap(), [reference[1].clone()]);
        let long = espeak.phonemize("fr", &["été ".repeat(300)]).unwrap();
        let once = espeak.phonemize("fr", &strings(&["été"])).unwrap();
        assert!(long[0].split(' ').all(|w| w == once[0]), "aucun octet égaré");
    }

    /// Le vrai espeak-ng : un mot qui répond comme la sentinelle et un mot de
    /// clôture dans le même lot ne déplacent aucune réponse.
    #[test]
    #[ignore]
    fn the_real_espeak_never_misattributes_answers_in_english() {
        let mut espeak = EspeakProcess::locate().expect("espeak-ng construit");
        let hello = espeak.phonemize("en", &strings(&["hello"])).unwrap();
        let xq = espeak.phonemize("en", &strings(&["Xq"])).unwrap();
        for _ in 0..20 {
            // Une erreur serait acceptable ; un succès doit être juste.
            if let Ok(out) = espeak.phonemize("en", &strings(&["Zyzomies", "hello", "Xq"])) {
                assert!(!out[0].is_empty());
                assert_eq!([&out[1], &out[2]], [&hello[0], &xq[0]]);
            }
            let again = espeak.phonemize("en", &strings(&["hello", "XQ", "xq"])).unwrap();
            assert_eq!(again, [hello[0].clone(), xq[0].clone(), xq[0].clone()]);
        }
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

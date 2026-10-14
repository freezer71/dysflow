//! Ordonnanceur de synthèse : plusieurs sessions en parallèle, pilotées par la
//! position de lecture.
//!
//! Le point clé est que les workers ne synthétisent pas le document du début à
//! la fin, mais **une fenêtre glissante autour du curseur**. Trois propriétés en
//! découlent :
//!
//! * un saut ré-oriente immédiatement la synthèse, ce qui rend le seek possible ;
//! * la mémoire reste bornée, là où une file non bornée gardait tout le PCM ;
//! * l'audio est produit à 1× et sert à toutes les vitesses : la vitesse
//!   s'applique à la lecture (`audio::tempo`), un changement ne touche pas
//!   au cache.

use crate::sync::{CondvarExt, LockExt};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Cancel, PcmBuffer, Synthesis, TtsEngine, TtsParams, WordSpan};
use crate::audio::pcm::{fade_edge, Edge};

/// Phrase à synthétiser.
#[derive(Debug, Clone)]
struct Job {
    text: String,
    /// Formes parlées des mots affichés, pour le minutage.
    words: Vec<String>,
    /// Formes écrites des mêmes mots (« 18 » là où `words` dit
    /// « dix-huit ») : la coupe sans ponctuation se décide dessus.
    written: Vec<String>,
    pause_ms: u64,
}

/// Ce qu'un worker prend : la phrase, ses mots, et sous quels paramètres.
struct Picked {
    id: usize,
    text: String,
    words: Vec<String>,
    written: Vec<String>,
    params: TtsParams,
    generation: u64,
    pause_ms: u64,
    /// La phrase attendue, à couper en tête et suite si elle s'y prête.
    split: bool,
}

/// Une phrase coupée en tête et suite.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Split {
    pub head_text: String,
    pub head_words: Vec<String>,
    pub tail_text: String,
    pub tail_words: Vec<String>,
}

/// Une phrase sans signe de coupe se synthétisait entière (~300 ms pour
/// dix mots avant le premier son) : elle est coupée après ce nombre de mots.
const WORD_CUT_HEAD: usize = 5;
/// Au plus loin, la coupe sans ponctuation tombe après ce nombre de mots.
const WORD_CUT_MAX_HEAD: usize = 7;
/// Une suite plus courte ne vaut pas la coupe.
const WORD_CUT_MIN_TAIL: usize = 4;
/// En dessous, la phrase reste entière : `WORD_CUT_HEAD + WORD_CUT_MIN_TAIL`.
const WORD_CUT_MIN_WORDS: usize = WORD_CUT_HEAD + WORD_CUT_MIN_TAIL;
/// Une tête qui finit sur l'un de ces mots laisse son complément en suspens
/// (« vers | la gare ») : la coupe avance d'un mot.
const FUNCTION_WORDS: &[&str] = &[
    "le", "la", "les", "l'", "un", "une", "des", "de", "du", "d'", "au", "aux", "à", "en", "vers", "et", "ou",
    "que", "qui", "pour", "par", "sur", "dans", "the", "a", "an", "of", "to", "and", "or", "in", "on", "for",
    "with",
];

/// Nombre de mots de la tête coupée à la ponctuation : après le premier
/// mot qui finit par une virgule, un point-virgule ou deux-points et laisse
/// au moins trois mots en tête et une suite non vide. Un signe trop tôt
/// (« Oui, … ») cède la place au suivant.
fn punctuation_cut(words: &[String]) -> Option<usize> {
    words
        .iter()
        .enumerate()
        .position(|(i, w)| w.ends_with([',', ';', ':']) && i >= 2 && i + 1 < words.len())
        .map(|i| i + 1)
}

/// Têtes possibles d'une coupe sans ponctuation, dans l'ordre : après le
/// cinquième mot, ou un peu plus loin (au plus le septième) si la coupe
/// tomberait après une élision, un nombre en chiffres ou un mot-outil. La
/// suite garde toujours `WORD_CUT_MIN_TAIL` mots. Décidé sur les formes
/// écrites : « 18 » s'y lit encore comme un nombre.
fn word_cuts(written: &[String]) -> impl Iterator<Item = usize> + '_ {
    let long_enough = written.len() >= WORD_CUT_MIN_WORDS;
    (WORD_CUT_HEAD..=WORD_CUT_MAX_HEAD)
        .filter(move |_| long_enough)
        .take_while(move |&k| written.len() - k >= WORD_CUT_MIN_TAIL)
        .filter(move |&k| may_end_a_head(&written[k - 1]))
}

/// Un mot écrit après lequel la coupe peut tomber : ni élision, ni nombre en
/// chiffres, ni mot-outil.
fn may_end_a_head(word: &str) -> bool {
    if word.ends_with(['\'', '’']) {
        return false;
    }
    let bare = word.trim_end_matches(|c: char| !c.is_alphanumeric());
    if bare.ends_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    !FUNCTION_WORDS.contains(&bare.to_lowercase().as_str())
}

fn split_at(words: &[String], cut: usize) -> Split {
    let (head, tail) = words.split_at(cut);
    Split {
        head_text: head.join(" "),
        head_words: head.to_vec(),
        tail_text: tail.join(" "),
        tail_words: tail.to_vec(),
    }
}

/// Coupe la phrase à envoyer au moteur : la coupe se décide sur les mots,
/// mais tête et suite sont prises dans `text`, la forme parlée de la phrase
/// entière. Un mot verbalisé seul perd son contexte (« 10 000 » dirait
/// « dix zéro zéro zéro ») : la phrase doit se dire de la même façon, coupée
/// ou non. Faute de retrouver la coupe dans `text`, la phrase reste entière.
///
/// `words` : formes parlées (minutage, ponctuation) ; `written` : formes
/// écrites, de même longueur, pour la coupe sans ponctuation.
pub(crate) fn split_sentence(text: &str, words: &[String], written: &[String]) -> Option<Split> {
    if let Some(cut) = punctuation_cut(words) {
        return split_text_at_mark(text, split_at(words, cut));
    }
    let written = if written.len() == words.len() { written } else { words };
    // Un nombre dit en plusieurs mots peut rendre une coupe introuvable dans
    // le texte : la suivante a sa chance, jusqu'au septième mot.
    word_cuts(written).find_map(|cut| split_text_at_word(text, split_at(words, cut)))
}

fn split_text_at_mark(text: &str, split: Split) -> Option<Split> {
    // Les signes de coupe suivis d'un blanc (ou finaux), dans la tête : le
    // dernier est la coupe ; le texte est coupé à son homologue.
    let rank = cut_marks(&split.head_text).count();
    let (at, mark) = cut_marks(text).nth(rank.checked_sub(1)?)?;
    let head = text[..at + mark.len_utf8()].trim();
    let tail = text[at + mark.len_utf8()..].trim();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    // La forme parlée peut ajouter des virgules que les mots n'ont pas (un
    // numéro dicté par groupes : « zéro six, douze, … ») : la coupe trouvée
    // dans le texte n'est sûre que si la tête finit par le dernier mot de la
    // tête. Sinon la phrase reste entière plutôt que coupée en plein nombre.
    if !split.head_words.last().is_some_and(|last| head.ends_with(last.as_str())) {
        return None;
    }
    Some(Split { head_text: head.to_string(), tail_text: tail.to_string(), ..split })
}

/// Coupe sans ponctuation : le texte est coupé au blanc qui sépare le dernier
/// mot de la tête du premier mot de la suite, mots entiers (une forme parlée
/// peut en compter plusieurs : « deux mille vingt-sept »). La même paire
/// peut revenir plus tôt dans la phrase : on prend son homologue de même
/// rang. Introuvable, pas de coupe.
fn split_text_at_word(text: &str, split: Split) -> Option<Split> {
    let last: Vec<&str> = split.head_words.last()?.split_whitespace().collect();
    let first: Vec<&str> = split.tail_words.first()?.split_whitespace().collect();
    if last.is_empty() || first.is_empty() {
        return None;
    }
    // Frontières (en jetons) où `last` finit et `first` commence.
    let boundaries = |tokens: &[&str]| -> Vec<usize> {
        (last.len()..=tokens.len().saturating_sub(first.len()))
            .filter(|&i| tokens[i - last.len()..i] == last[..] && tokens[i..i + first.len()] == first[..])
            .collect()
    };
    let head_tokens = split.head_words.iter().flat_map(|w| w.split_whitespace()).count();
    let spoken: Vec<&str> =
        split.head_words.iter().chain(&split.tail_words).flat_map(|w| w.split_whitespace()).collect();
    let rank = boundaries(&spoken).iter().position(|&i| i == head_tokens)?;
    // Début de chaque jeton de `text`, pour couper au blanc qui précède.
    let starts: Vec<usize> = text
        .char_indices()
        .filter(|&(i, c)| !c.is_whitespace() && text[..i].chars().next_back().is_none_or(char::is_whitespace))
        .map(|(i, _)| i)
        .collect();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let at = starts[*boundaries(&tokens).get(rank)?];
    let head = text[..at].trim();
    let tail = text[at..].trim();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    Some(Split { head_text: head.to_string(), tail_text: tail.to_string(), ..split })
}

/// Position des virgules, points-virgules et deux-points qui finissent un
/// mot (suivis d'un blanc, ou en fin de texte).
fn cut_marks(text: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    text.char_indices().filter(move |&(i, c)| {
        matches!(c, ',' | ';' | ':')
            && text[i + c.len_utf8()..].chars().next().is_none_or(char::is_whitespace)
    })
}

/// Quelle partie d'une phrase un audio porte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// La phrase entière.
    Whole,
    /// La tête de la phrase attendue ; la suite vient par `Synthesizer::tail`.
    Head { words: usize },
}

/// Audio prêt à jouer pour une phrase.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Parole suivie du silence de fin de bloc.
    pub audio: Arc<PcmBuffer>,
    /// Durée de la parole seule, hors silence.
    ///
    /// Le curseur de mots s'appuie dessus : inclure le silence ferait traîner le
    /// dernier mot jusqu'au bloc suivant.
    pub speech: Duration,
    /// Temps de chaque mot affiché, quand le moteur le donne.
    pub words: Option<Arc<[WordSpan]>>,
    pub part: Part,
}

/// Compteur affiché dans la barre d'état.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SynthesisStats {
    pub ready: usize,
    pub in_flight: usize,
    pub failed: usize,
    pub total: usize,
    pub workers: usize,
    /// Temps passé à synthétiser, cumulé sur toutes les sessions.
    pub busy: Duration,
}

struct Entry {
    rendered: Rendered,
    /// La suite d'une phrase coupée, quand elle est prête.
    tail: Option<Rendered>,
    generation: u64,
}

struct State {
    jobs: Vec<Job>,
    params: TtsParams,
    /// Incrémentée à chaque changement de paramètres ; les résultats produits
    /// sous une génération périmée sont jetés.
    generation: u64,
    cursor: usize,
    window: usize,
    keep_behind: usize,
    cache: HashMap<usize, Entry>,
    in_flight: HashSet<usize>,
    failures: HashMap<usize, String>,
    /// La dernière ouverture de session a échoué, avec ce message. Effacé
    /// par une ouverture réussie ; gardé d'un texte à l'autre, le moteur
    /// restant le même.
    open_failure: Option<String>,
    /// Sessions ouvertes, tous workers confondus : tant qu'il y en a une, la
    /// voix parle, quoi qu'ait dit la dernière ouverture.
    open_sessions: usize,
    /// Ouvertures en cours : un worker refusé pendant qu'un autre ouvre
    /// encore lui laisse la phrase.
    opening: usize,
    /// Temps passé à synthétiser, cumulé.
    busy: Duration,
}

impl State {
    /// Prochaine phrase à synthétiser, au plus près du curseur.
    ///
    /// `split_cursor` : le moteur demande, à cet instant, que la phrase
    /// attendue (au curseur) soit coupée en deux pour sonner plus tôt.
    fn next_job(&self, split_cursor: bool) -> Option<Picked> {
        let end = (self.cursor + self.window).min(self.jobs.len());
        for id in self.cursor..end {
            if self.in_flight.contains(&id) || self.failures.contains_key(&id) {
                continue;
            }
            if self.cache.get(&id).is_some_and(|e| e.generation == self.generation) {
                continue;
            }
            let job = &self.jobs[id];
            return Some(Picked {
                id,
                text: job.text.clone(),
                words: job.words.clone(),
                written: job.written.clone(),
                params: self.params.clone(),
                generation: self.generation,
                pause_ms: job.pause_ms,
                split: split_cursor && id == self.cursor,
            });
        }
        None
    }

    /// Vrai tant qu'une synthèse de la phrase `id` sous `generation` a un
    /// usage : même génération, et phrase dans la fenêtre
    /// [curseur, curseur + fenêtre) — celle où `next_job` puise. La suite
    /// d'une phrase que le curseur a quittée n'en a plus.
    fn still_wanted(&self, id: usize, generation: u64) -> bool {
        generation == self.generation && id >= self.cursor && id < self.cursor + self.window && id < self.jobs.len()
    }

    /// Libère ce qui est trop loin du curseur pour resservir.
    fn evict(&mut self) {
        let low = self.cursor.saturating_sub(self.keep_behind);
        let high = self.cursor + self.window * 2;
        self.cache.retain(|&id, _| id >= low && id <= high);
    }
}

struct Shared {
    engine: Arc<dyn TtsEngine>,
    state: Mutex<State>,
    wake: Condvar,
    shutdown: AtomicBool,
}

pub struct Synthesizer {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
    worker_count: usize,
}

impl Synthesizer {
    /// Démarre `workers` threads de synthèse sur `engine`.
    pub fn new(
        engine: Arc<dyn TtsEngine>,
        params: TtsParams,
        workers: usize,
        window: usize,
    ) -> Self {
        let worker_count = workers.clamp(1, engine.max_sessions().max(1));
        let shared = Arc::new(Shared {
            engine,
            state: Mutex::new(State {
                jobs: Vec::new(),
                params,
                generation: 1,
                cursor: 0,
                window: window.max(2),
                keep_behind: 8,
                cache: HashMap::new(),
                in_flight: HashSet::new(),
                failures: HashMap::new(),
                open_failure: None,
                open_sessions: 0,
                opening: 0,
                busy: Duration::ZERO,
            }),
            wake: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });

        let handles = (0..worker_count)
            .map(|_| {
                let shared = Arc::clone(&shared);
                crate::sync::spawn_named("sillon-synthese", move || worker_loop(shared))
            })
            .collect();

        Self {
            shared,
            workers: handles,
            worker_count,
        }
    }

    /// Ajoute une phrase à la fin du document, sans mots à minuter.
    pub fn push(&self, text: String, pause_ms: u64) {
        self.push_with_words(text, Vec::new(), pause_ms);
    }

    /// Ajoute une phrase et les formes parlées de ses mots affichés.
    pub fn push_with_words(&self, text: String, words: Vec<String>, pause_ms: u64) {
        let written = words.clone();
        self.push_sentence(text, words, written, pause_ms);
    }

    /// Ajoute une phrase, les formes parlées de ses mots et leurs formes
    /// écrites, sur lesquelles se décide la coupe sans ponctuation : un
    /// nombre en chiffres y reste reconnaissable.
    pub fn push_sentence(&self, text: String, words: Vec<String>, written: Vec<String>, pause_ms: u64) {
        let written = if written.len() == words.len() { written } else { words.clone() };
        let mut state = self.shared.state.locked();
        state.jobs.push(Job { text, words, written, pause_ms });
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Déplace la fenêtre de synthèse.
    pub fn set_cursor(&self, cursor: usize) {
        let mut state = self.shared.state.locked();
        if state.cursor == cursor {
            return;
        }
        state.cursor = cursor;
        state.evict();
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Change les paramètres de synthèse et invalide ce qui a été produit.
    ///
    /// Voix, langue, échantillonnage… La vitesse n'en fait pas partie : elle
    /// s'applique au son, et ne vide jamais le cache.
    pub fn set_params(&self, params: TtsParams) {
        let mut state = self.shared.state.locked();
        if state.params == params {
            return;
        }
        state.params = params;
        state.generation += 1;
        state.cache.clear();
        state.failures.clear();
        drop(state);
        self.shared.wake.notify_all();
    }

    pub fn params(&self) -> TtsParams {
        self.shared.state.locked().params.clone()
    }

    /// Audio d'une phrase, s'il est prêt et à jour.
    pub fn get(&self, id: usize) -> Option<Rendered> {
        let state = self.shared.state.locked();
        state
            .cache
            .get(&id)
            .filter(|e| e.generation == state.generation)
            .map(|e| e.rendered.clone())
    }

    /// La suite d'une phrase coupée, si elle est prête et à jour. Son
    /// minutage part du début de la suite.
    pub fn tail(&self, id: usize) -> Option<Rendered> {
        let state = self.shared.state.locked();
        state
            .cache
            .get(&id)
            .filter(|e| e.generation == state.generation)
            .and_then(|e| e.tail.clone())
    }

    /// Message d'erreur si la synthèse de cette phrase a échoué.
    pub fn failure(&self, id: usize) -> Option<String> {
        self.shared.state.locked().failures.get(&id).cloned()
    }

    /// Message de la dernière ouverture de session, si elle a échoué et
    /// qu'aucune session n'est ouverte : la voix entière est alors muette.
    /// Une session ouverte ailleurs parle : rien à signaler.
    pub fn open_failure(&self) -> Option<String> {
        let state = self.shared.state.locked();
        if state.open_sessions > 0 {
            return None;
        }
        state.open_failure.clone()
    }

    /// Redonne leur chance aux phrases en échec à partir de `from`.
    ///
    /// Un refus du moteur condamnait la phrase pour la session entière : y
    /// revenir exprès ne la faisait pas réentendre. Or c'est précisément le
    /// geste de quelqu'un qui veut réécouter un passage.
    pub fn retry_from(&self, from: usize) {
        let mut state = self.shared.state.locked();
        let before = state.failures.len();
        state.failures.retain(|&id, _| id < from);
        if state.failures.len() == before {
            return;
        }
        drop(state);
        self.shared.wake.notify_all();
    }

    /// Nom courant du moteur.
    ///
    /// Interrogé à chaque trame : il évolue pendant le chargement différé.
    pub fn engine_name(&self) -> String {
        self.shared.engine.name()
    }

    /// Vrai si le moteur exige un WAV de référence pour parler.
    /// Le moteur, pour ouvrir une session hors de l'ordonnanceur.
    ///
    /// Sert aux aperçus : ils doivent parler avec le même modèle et les mêmes
    /// poids que la lecture, sans pour autant occuper un de ses workers.
    pub fn engine(&self) -> Arc<dyn TtsEngine> {
        Arc::clone(&self.shared.engine)
    }

    /// Repart d'un document vide, threads et sessions gardés ouverts.
    ///
    /// Les synthèses encore en cours portent l'ancienne génération : elles
    /// seront jetées à leur arrivée, et libéreront alors leur place.
    pub fn reset(&self) {
        let mut state = self.shared.state.locked();
        state.jobs.clear();
        state.cache.clear();
        state.failures.clear();
        state.cursor = 0;
        state.generation += 1;
        drop(state);
        self.shared.wake.notify_all();
    }

    pub fn stats(&self) -> SynthesisStats {
        let state = self.shared.state.locked();
        SynthesisStats {
            ready: state
                .cache
                .values()
                .filter(|e| e.generation == state.generation)
                .count(),
            in_flight: state.in_flight.len(),
            failed: state.failures.len(),
            total: state.jobs.len(),
            workers: self.worker_count,
            busy: state.busy,
        }
    }

}

impl Drop for Synthesizer {
    fn drop(&mut self) {
        // Le drapeau se lève sous le verrou. Levé à côté, il pouvait tomber
        // entre la vérification d'un worker et son endormissement : le réveil
        // partait dans le vide, et la fermeture attendait un thread que plus
        // rien ne réveillerait — « q » restait suspendu.
        {
            let _state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            self.shared.shutdown.store(true, Ordering::SeqCst);
            self.shared.wake.notify_all();
        }
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(shared: Arc<Shared>) {
    // La session est ouverte au premier travail : un moteur qui échoue à
    // s'initialiser ne doit pas empêcher le lecteur de démarrer.
    let mut session = None;

    loop {
        let job = {
            let mut state = shared.state.locked();
            loop {
                if shared.shutdown.load(Ordering::SeqCst) {
                    if session.is_some() {
                        state.open_sessions -= 1;
                    }
                    return;
                }
                // Demandé à chaque choix : un moteur chargé en différé ne le
                // sait qu'une fois prêt.
                if let Some(job) = state.next_job(shared.engine.splits_first_clause()) {
                    state.in_flight.insert(job.id);
                    break job;
                }
                state = shared.wake.wait_locked(state);
            }
        };

        if session.is_none() {
            shared.state.locked().opening += 1;
            let opened = shared.engine.open_session();
            shared.state.locked().opening -= 1;
            match opened {
                Ok(s) => {
                    session = Some(s);
                    let mut state = shared.state.locked();
                    state.open_failure = None;
                    state.open_sessions += 1;
                }
                Err(err) => {
                    // Une famille qui ne sait pas synthétiser échoue ici, à
                    // chaque phrase : le lecteur doit pouvoir le dire.
                    let mut state = shared.state.locked();
                    state.open_failure = Some(err.to_string());
                    if state.open_sessions > 0 || state.opening > 0 {
                        // Une autre session parle (ou s'ouvre) : ce worker se
                        // retire et rend la phrase, plutôt que de la faire
                        // sauter. Si l'autre échoue aussi, c'est lui qui le
                        // dira.
                        state.in_flight.remove(&job.id);
                        drop(state);
                        shared.wake.notify_all();
                        return;
                    }
                    drop(state);
                    finish_generation(
                        &shared,
                        job.id,
                        job.generation,
                        Err(format!("ouverture de session : {err}")),
                    );
                    continue;
                }
            }
        }

        let session = session.as_mut().expect("session ouverte juste au-dessus");
        let cancel = cancel_for(&shared, job.id, job.generation);
        let split = if job.split { split_sentence(&job.text, &job.words, &job.written) } else { None };
        let Some(split) = split else {
            match synthesize(&shared, session.as_mut(), &job.text, &job.words, &job.params, &cancel) {
                Err(Stop::Cancelled) => abandon(&shared, job.id, job.generation),
                outcome => {
                    let outcome = outcome
                        .map(|s| with_trailing_silence(s, job.words.len(), job.pause_ms))
                        .map_err(Stop::message);
                    finish_generation(&shared, job.id, job.generation, outcome);
                }
            }
            continue;
        };
        // La tête d'abord, jouable aussitôt ; la phrase reste « en cours »
        // jusqu'à la suite.
        let head = synthesize(&shared, session.as_mut(), &split.head_text, &split.head_words, &job.params, &cancel)
            .map(|s| {
                let s = Synthesis { pcm: fade_edge(s.pcm, Edge::End), ..s };
                let mut rendered = with_trailing_silence(s, split.head_words.len(), 0);
                rendered.part = Part::Head { words: split.head_words.len() };
                rendered
            });
        match head {
            Ok(rendered) => finish_head(&shared, job.id, job.generation, rendered),
            Err(Stop::Cancelled) => {
                abandon(&shared, job.id, job.generation);
                continue;
            }
            Err(Stop::Failed(message)) => {
                finish_generation(&shared, job.id, job.generation, Err(message));
                continue;
            }
        }
        match synthesize(&shared, session.as_mut(), &split.tail_text, &split.tail_words, &job.params, &cancel) {
            Err(Stop::Cancelled) => abandon(&shared, job.id, job.generation),
            tail => {
                let tail = tail
                    .map(|s| Synthesis { pcm: fade_edge(s.pcm, Edge::Start), ..s })
                    .map(|s| with_trailing_silence(s, split.tail_words.len(), job.pause_ms))
                    .map_err(Stop::message);
                finish_tail(&shared, job.id, job.generation, tail);
            }
        }
    }
}

/// Pourquoi une synthèse n'a pas rendu de son.
#[derive(Debug)]
enum Stop {
    /// Devenue inutile en route : ni échec ni résultat.
    Cancelled,
    Failed(String),
}

impl Stop {
    /// Le message d'un échec ; un abandon est traité avant.
    fn message(self) -> String {
        match self {
            Stop::Cancelled => super::Cancelled.to_string(),
            Stop::Failed(message) => message,
        }
    }
}

/// Le test d'abandon de la phrase `id` prise sous `generation` : il prend
/// un instant le verrou du pool, jamais pendant le calcul lui-même (le
/// moteur l'appelle entre deux étapes). La fermeture du pool abandonne
/// aussi.
fn cancel_for(shared: &Arc<Shared>, id: usize, generation: u64) -> Cancel {
    let shared = Arc::clone(shared);
    Cancel::when(move || {
        if shared.shutdown.load(Ordering::SeqCst) {
            return true;
        }
        // Appelé depuis le calcul de l'app (Swift) : jamais de panique ici.
        let state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        !state.still_wanted(id, generation)
    })
}

/// Une synthèse abandonnée : la phrase n'est plus en cours, rien n'est
/// rangé ni compté comme échec. Abandonnée pendant sa suite, sa tête part
/// aussi : restée seule en cache, elle empêcherait de refaire la phrase.
///
/// Course bénigne : le curseur peut revenir sur la phrase entre la décision
/// du moteur et ce verrou. La phrase est alors simplement refaite (au pire
/// une tête de plus) : rien n'est perdu ni joué deux fois.
fn abandon(shared: &Arc<Shared>, id: usize, generation: u64) {
    let mut state = shared.state.locked();
    state.in_flight.remove(&id);
    if generation == state.generation {
        state.cache.remove(&id);
    }
    drop(state);
    shared.wake.notify_all();
}

/// Une synthèse, chronométrée : le temps occupé est la mesure du filtre de
/// charge. Un son vide est un échec. Une synthèse abandonnée compte dans
/// `busy` pour le temps réellement passé.
fn synthesize(
    shared: &Arc<Shared>,
    session: &mut dyn super::TtsSession,
    text: &str,
    words: &[String],
    params: &TtsParams,
    cancel: &Cancel,
) -> Result<Synthesis, Stop> {
    let started = std::time::Instant::now();
    let outcome = session.synthesize_cancellable(text, words, params, cancel);
    shared.state.locked().busy += started.elapsed();
    match outcome {
        Ok(s) if s.pcm.is_empty() => Err(Stop::Failed("aucun échantillon produit".to_string())),
        Ok(s) => Ok(s),
        Err(e) if super::is_cancelled(&e) => Err(Stop::Cancelled),
        Err(e) => Err(Stop::Failed(e.to_string())),
    }
}

/// Ajoute le silence de fin de bloc à la parole.
///
/// Ce silence porte la hiérarchie à l'oreille : une pause plus longue après un
/// titre qu'après un item de liste, sans jamais prononcer le moindre marqueur.
/// Un minutage qui ne couvre pas exactement les mots est écarté.
fn with_trailing_silence(synthesis: Synthesis, expected_words: usize, pause_ms: u64) -> Rendered {
    let speech = synthesis.pcm;
    let words = synthesis
        .words
        .filter(|w| expected_words > 0 && w.len() == expected_words)
        .map(Arc::from);
    let speech_duration = speech.duration;
    if pause_ms == 0 {
        return Rendered { audio: Arc::new(speech), speech: speech_duration, words, part: Part::Whole };
    }
    let frames = (speech.sample_rate as u64 * pause_ms / 1000) as usize;
    let mut samples = Vec::with_capacity(speech.samples.len() + frames * speech.channels as usize);
    samples.extend_from_slice(&speech.samples);
    samples.resize(samples.len() + frames * speech.channels as usize, 0.0);
    Rendered {
        audio: Arc::new(PcmBuffer::new(samples, speech.sample_rate, speech.channels)),
        speech: speech_duration,
        words,
        part: Part::Whole,
    }
}

fn finish_generation(
    shared: &Arc<Shared>,
    id: usize,
    generation: u64,
    outcome: Result<Rendered, String>,
) {
    let mut state = shared.state.locked();
    state.in_flight.remove(&id);
    // Un résultat produit sous d'anciens paramètres n'a plus lieu d'être joué.
    if generation == state.generation {
        match outcome {
            Ok(rendered) => {
                state.cache.insert(id, Entry { rendered, tail: None, generation });
                state.evict();
            }
            Err(message) => {
                state.failures.insert(id, message);
            }
        }
    }
    drop(state);
    shared.wake.notify_all();
}

/// Range la tête d'une phrase coupée ; la phrase reste en cours.
fn finish_head(shared: &Arc<Shared>, id: usize, generation: u64, rendered: Rendered) {
    let mut state = shared.state.locked();
    if generation == state.generation {
        state.cache.insert(id, Entry { rendered, tail: None, generation });
    }
    drop(state);
    shared.wake.notify_all();
}

/// Range la suite d'une phrase coupée. Un échec de la suite se dit comme
/// celui d'une phrase : la lecture passe à la suivante. La tête part avec
/// elle : restée seule en cache, elle empêcherait de refaire la phrase.
fn finish_tail(shared: &Arc<Shared>, id: usize, generation: u64, outcome: Result<Rendered, String>) {
    let mut state = shared.state.locked();
    state.in_flight.remove(&id);
    if generation == state.generation {
        match outcome {
            Ok(rendered) => {
                if let Some(entry) = state.cache.get_mut(&id) {
                    entry.tail = Some(rendered);
                }
            }
            Err(message) => {
                state.cache.remove(&id);
                state.failures.insert(id, message);
            }
        }
    }
    drop(state);
    shared.wake.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use crate::tts::TtsSession;
    use std::sync::atomic::AtomicUsize;

    /// Moteur déterministe : la durée suit le nombre de mots.
    struct FakeEngine {
        calls: Arc<AtomicUsize>,
        concurrent: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    struct FakeSession {
        calls: Arc<AtomicUsize>,
        concurrent: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    impl TtsEngine for FakeEngine {
        fn name(&self) -> String {
            "fake".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(FakeSession {
                calls: Arc::clone(&self.calls),
                concurrent: Arc::clone(&self.concurrent),
                peak: Arc::clone(&self.peak),
            }))
        }
        fn max_sessions(&self) -> usize {
            8
        }
    }

    impl TtsSession for FakeSession {
        fn synthesize(&mut self, text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            let now = self.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(30));
            self.concurrent.fetch_sub(1, Ordering::SeqCst);

            // 1000 échantillons par mot.
            let words = text.split_whitespace().count().max(1);
            let count = 1000 * words;
            Ok(PcmBuffer::new(vec![0.1; count], 1000, 1))
        }
    }

    /// Moteur qui répond sans attendre : les workers enchaînent alors
    /// prise de travail et sommeil aussi vite que possible.
    struct InstantEngine;
    struct InstantSession;

    impl TtsEngine for InstantEngine {
        fn name(&self) -> String {
            "instantané".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(InstantSession))
        }
        fn max_sessions(&self) -> usize {
            8
        }
    }

    impl TtsSession for InstantSession {
        fn synthesize(&mut self, _text: &str, _params: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.1; 10], 1000, 1))
        }
    }

    /// Compte les sessions ouvertes, et rend un son proportionnel au texte.
    struct OpeningCounter {
        opened: Arc<AtomicUsize>,
    }
    struct LengthSession;

    impl TtsEngine for OpeningCounter {
        fn name(&self) -> String {
            "compteur".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            self.opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(LengthSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for LengthSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    fn wait_ready(synth: &Synthesizer, id: usize) -> Rendered {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = synth.get(id) {
                return r;
            }
            assert!(std::time::Instant::now() < deadline, "phrase {id} jamais prête");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn reset_keeps_the_session_and_forgets_the_text() {
        let opened = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(OpeningCounter { opened: Arc::clone(&opened) });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 40);

        synth.reset();
        assert_eq!(synth.stats().total, 0);
        synth.push("bb".into(), 0);
        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    /// Moteur dont la première ouverture de session attend un feu vert, puis
    /// échoue ; les suivantes réussissent.
    struct SlowFailingOpen {
        opens: AtomicUsize,
        started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
        gate: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl TtsEngine for SlowFailingOpen {
        fn name(&self) -> String {
            "ouverture lente".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            if self.opens.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.locked().send(()).unwrap();
                self.gate.locked().recv().unwrap();
                anyhow::bail!("refusée");
            }
            Ok(Box::new(LengthSession))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    #[test]
    fn a_failed_open_for_the_old_text_does_not_poison_the_new_one() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let engine = Arc::new(SlowFailingOpen {
            opens: AtomicUsize::new(0),
            started: std::sync::Mutex::new(started_tx),
            gate: std::sync::Mutex::new(gate),
        });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        synth.reset();
        synth.push("bb".into(), 0);
        release.send(()).unwrap();

        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(synth.failure(0), None);
        assert_eq!(synth.open_failure(), None, "une ouverture réussie efface l'échec");
    }

    /// Moteur chargé dont aucune session ne s'ouvre (une famille qui ne
    /// sait ni synthétiser ni cloner hors ligne).
    struct NoSession;

    impl TtsEngine for NoSession {
        fn name(&self) -> String {
            "sans session".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            anyhow::bail!("cette famille ne sait ni synthétiser ni cloner hors-ligne")
        }
        fn max_sessions(&self) -> usize {
            2
        }
    }

    #[test]
    fn a_session_that_never_opens_is_said_once_for_the_voice() {
        let synth = Synthesizer::new(Arc::new(NoSession), TtsParams::default(), 2, 4);
        assert_eq!(synth.open_failure(), None, "rien n'a encore échoué");
        synth.push("Une phrase.".into(), 0);
        synth.push("Une autre.".into(), 0);
        assert!(wait_for(|| synth.failure(1).is_some()));
        assert_eq!(
            synth.open_failure().as_deref(),
            Some("cette famille ne sait ni synthétiser ni cloner hors-ligne")
        );
        synth.reset();
        assert!(synth.open_failure().is_some(), "le moteur reste le même d'un texte à l'autre");
    }

    /// La première session s'ouvre, les suivantes échouent (un moteur à
    /// plusieurs sessions, l'une refusée).
    struct OneSessionOnly {
        opens: AtomicUsize,
    }

    impl TtsEngine for OneSessionOnly {
        fn name(&self) -> String {
            "une session".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            if self.opens.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(Box::new(SlowSession));
            }
            anyhow::bail!("plus de session")
        }
        fn max_sessions(&self) -> usize {
            2
        }
    }

    /// Lente assez pour que l'autre worker prenne une phrase entre-temps.
    struct SlowSession;
    impl TtsSession for SlowSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            thread::sleep(Duration::from_millis(30));
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    #[test]
    fn one_open_session_keeps_the_voice_available() {
        let engine = Arc::new(OneSessionOnly { opens: AtomicUsize::new(0) });
        let synth = Synthesizer::new(Arc::clone(&engine) as Arc<dyn TtsEngine>, TtsParams::default(), 2, 4);
        for i in 0..6 {
            synth.push(format!("phrase {i}"), 0);
        }
        assert!(wait_for(|| engine.opens.load(Ordering::SeqCst) >= 2), "les deux workers ont essayé");
        for i in 0..4 {
            synth.set_cursor(i);
            wait_ready(&synth, i);
        }
        assert_eq!(synth.open_failure(), None, "une session parle : la voix n'est pas indisponible");
        assert_eq!(synth.stats().failed, 0, "aucune phrase sautée pour le worker refusé");
    }

    #[test]
    fn a_sentence_refused_by_an_open_session_never_makes_the_voice_unavailable() {
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = Arc::new(SplittingEngine::new(&texts));
        engine.fail_tail.store(true, Ordering::SeqCst);
        let synth = Synthesizer::new(engine as Arc<dyn TtsEngine>, TtsParams::default(), 1, 4);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        synth.push_with_words(first.into(), words(first), 0);
        assert!(wait_for(|| synth.failure(0).is_some()));
        assert_eq!(synth.open_failure(), None);
    }

    /// Moteur dont chaque synthèse annonce son départ puis attend un feu vert.
    struct GatedSynthesis {
        calls: Arc<AtomicUsize>,
        started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }
    struct GatedSession {
        calls: Arc<AtomicUsize>,
        started: std::sync::mpsc::Sender<()>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl TtsEngine for GatedSynthesis {
        fn name(&self) -> String {
            "à feu vert".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(GatedSession {
                calls: Arc::clone(&self.calls),
                started: self.started.locked().clone(),
                gate: Arc::clone(&self.gate),
            }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for GatedSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.started.send(()).unwrap();
            self.gate.locked().recv().unwrap();
            Ok(PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1))
        }
    }

    #[test]
    fn a_synthesis_in_flight_across_a_reset_is_dropped_and_the_id_reused() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(GatedSynthesis {
            calls: Arc::clone(&calls),
            started: std::sync::Mutex::new(started_tx),
            gate: Arc::new(std::sync::Mutex::new(gate)),
        });
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("aaaa".into(), 0);
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        synth.reset();
        synth.push("bb".into(), 0);
        release.send(()).unwrap();
        release.send(()).unwrap();

        assert_eq!(wait_ready(&synth, 0).audio.samples.len(), 20);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "une fois l'ancien, une fois le nouveau");
    }

    #[test]
    fn dropping_the_pool_never_hangs() {
        // Le réveil de fermeture se perdait quand il tombait entre la
        // vérification du drapeau par un worker et son endormissement : la
        // fermeture attendait alors un thread que plus rien ne réveillerait,
        // et « q » restait suspendu. La fenêtre est étroite : on la vise en
        // fermant des pools par milliers, workers en pleine activité.
        let (done, finished) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let engine: Arc<dyn TtsEngine> = Arc::new(InstantEngine);
            for _ in 0..60_000 {
                let synth = Synthesizer::new(Arc::clone(&engine), TtsParams::default(), 3, 4);
                for i in 0..4 {
                    synth.push(format!("phrase {i}"), 0);
                }
                drop(synth);
            }
            let _ = done.send(());
        });
        finished
            .recv_timeout(Duration::from_secs(30))
            .expect("la fermeture d'un pool est restée bloquée");
    }

    fn engine() -> (Arc<FakeEngine>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let engine = Arc::new(FakeEngine {
            calls: Arc::clone(&calls),
            concurrent: Arc::new(AtomicUsize::new(0)),
            peak: Arc::clone(&peak),
        });
        (engine, calls, peak)
    }

    fn wait_for(mut predicate: impl FnMut() -> bool) -> bool {
        for _ in 0..200 {
            if predicate() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn synthesizes_ahead_of_the_cursor() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        for i in 0..10 {
            synth.push(format!("phrase numéro {i}"), 0);
        }
        assert!(wait_for(|| synth.get(0).is_some() && synth.get(3).is_some()));
        // Rien n'est produit au-delà de la fenêtre.
        assert!(synth.get(9).is_none());
    }

    #[test]
    fn runs_sessions_in_parallel() {
        let (engine, _, peak) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 4, 8);
        for i in 0..8 {
            synth.push(format!("phrase {i}"), 0);
        }
        assert!(wait_for(|| synth.stats().ready >= 8));
        assert!(peak.load(Ordering::SeqCst) > 1, "la synthèse est restée séquentielle");
    }

    #[test]
    fn seeking_redirects_synthesis() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 3);
        for i in 0..40 {
            synth.push(format!("phrase {i}"), 0);
        }
        assert!(wait_for(|| synth.get(0).is_some()));
        assert!(synth.get(30).is_none());

        synth.set_cursor(30);
        assert!(wait_for(|| synth.get(30).is_some()), "le saut n'a pas réorienté la synthèse");
    }

    #[test]
    fn a_change_of_voice_settings_regenerates_the_window() {
        // Voix, langue, échantillonnage : le son change, le cache se refait.
        // (La vitesse, elle, n'est pas un paramètre de synthèse.)
        let (engine, calls, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        synth.push("un deux trois quatre".to_string(), 0);
        assert!(wait_for(|| synth.get(0).is_some()));
        let before = calls.load(Ordering::SeqCst);

        let mut params = synth.params();
        params.voice = "F2".into();
        synth.set_params(params);

        assert!(wait_for(|| calls.load(Ordering::SeqCst) > before && synth.get(0).is_some()));
    }

    #[test]
    fn memory_stays_bounded_while_reading_forward() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 2, 4);
        for i in 0..200 {
            synth.push(format!("phrase {i}"), 0);
        }
        for cursor in 0..60 {
            synth.set_cursor(cursor);
            thread::sleep(Duration::from_millis(5));
        }
        let cached = synth.shared.state.locked().cache.len();
        assert!(cached <= 32, "le cache a grossi sans borne : {cached}");
    }

    /// Un mot = 100 ms ; le minutage rendu suit les mots reçus. Avec
    /// `wrong`, il en oublie un.
    struct TimedEngine {
        wrong: bool,
    }
    struct TimedSession {
        wrong: bool,
    }

    impl TtsEngine for TimedEngine {
        fn name(&self) -> String {
            "minuté".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(TimedSession { wrong: self.wrong }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
    }

    impl TtsSession for TimedSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            let words = text.split_whitespace().count().max(1);
            Ok(PcmBuffer::new(vec![0.1; words * 100], 1000, 1))
        }
        fn synthesize_timed(&mut self, text: &str, words: &[String], p: &TtsParams) -> Result<super::super::Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let count = if self.wrong { words.len().saturating_sub(1) } else { words.len() };
            let spans = (0..count)
                .map(|i| crate::doc::WordSpan { start: i as f32 * 0.1, end: (i + 1) as f32 * 0.1 })
                .collect();
            Ok(super::super::Synthesis { pcm, words: Some(spans) })
        }
    }

    #[test]
    fn the_timing_of_each_word_reaches_the_rendered_audio() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: false }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux trois".into(), vec!["un".into(), "deux".into(), "trois".into()], 0);
        let rendered = wait_ready(&synth, 0);
        let words = rendered.words.expect("minutage rendu");
        assert_eq!(words.len(), 3);
        assert_eq!(words[2].end, 0.3);
        assert_eq!(rendered.part, Part::Whole);
    }

    #[test]
    fn a_timing_that_does_not_cover_the_words_is_dropped() {
        let synth = Synthesizer::new(Arc::new(TimedEngine { wrong: true }), TtsParams::default(), 1, 4);
        synth.push_with_words("un deux".into(), vec!["un".into(), "deux".into()], 0);
        assert!(wait_ready(&synth, 0).words.is_none());
    }

    #[test]
    fn the_time_spent_synthesizing_is_counted() {
        let (engine, _, _) = engine();
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        synth.push("une phrase".into(), 0);
        wait_ready(&synth, 0);
        assert!(synth.stats().busy >= Duration::from_millis(25), "{:?}", synth.stats().busy);
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    /// Une phrase passée par le vrai découpage : texte parlé, formes
    /// parlées et formes écrites, comme le lecteur les donne au pool.
    fn real_sentence(text: &str) -> crate::doc::Sentence {
        let mut builder = crate::doc::DocumentBuilder::new(crate::doc::builder::CodePolicy::Announce);
        builder.push_block(text);
        let doc = builder.document();
        assert_eq!(doc.sentences.len(), 1, "une seule phrase : {text}");
        doc.sentences[0].clone()
    }

    fn written(sentence: &crate::doc::Sentence) -> Vec<String> {
        sentence.words.iter().map(|w| w.text.clone()).collect()
    }

    fn split_real(text: &str) -> Option<Split> {
        let s = real_sentence(text);
        split_sentence(&s.text, &s.spoken_words(), &written(&s))
    }

    #[test]
    fn the_first_clause_is_cut_after_three_words_at_least() {
        let split = split_real("Le quatorze juillet, Camille harangua la foule.").unwrap();
        assert_eq!(split.head_text, "Le quatorze juillet,");
        assert_eq!(split.head_words.len(), 3);
        assert_eq!(split.tail_text, "Camille harangua la foule.");
        assert!(split_real("Une phrase sans virgule du tout.").is_none());
        assert!(split_real("Trois mots ici,").is_none(), "rien après");
    }

    #[test]
    fn a_comma_too_early_gives_way_to_a_later_one() {
        let split = split_real("Oui, bien sûr, tout à fait.").unwrap();
        assert_eq!(split.head_text, "Oui, bien sûr,");
        let split = split_real("Oui, le train partira vers midi depuis la gare, sans retard.").unwrap();
        assert_eq!(split.head_text, "Oui, le train partira vers midi depuis la gare,");
    }

    #[test]
    fn a_long_sentence_without_punctuation_is_cut_after_five_words() {
        let split = split_real("Camille traversa la grande place sous la pluie fine du matin.").unwrap();
        assert_eq!(split.head_text, "Camille traversa la grande place");
        assert_eq!(split.head_words.len(), 5);
        assert_eq!(split.tail_text, "sous la pluie fine du matin.");
        assert_eq!(split.tail_words.len(), 6);
    }

    #[test]
    fn a_ten_word_sentence_without_punctuation_gets_a_five_word_head() {
        let split = split_real("Il marchait très longtemps seul sans jamais se retourner vraiment.").unwrap();
        assert_eq!(split.head_words.len(), 5);
        assert_eq!(split.tail_words.len(), 5);
    }

    #[test]
    fn a_sentence_of_eight_words_without_punctuation_stays_whole() {
        assert!(split_real("Il marchait très longtemps seul sans fin aucune.").is_none());
    }

    #[test]
    fn the_word_cut_never_falls_after_an_elision() {
        // Écrit « qu' » seul au cinquième mot : la coupe avance d'un mot.
        let words: Vec<String> = "Elle savait bien alors qu' il ne viendrait plus jamais ce soir."
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let text = words.join(" ");
        let split = split_sentence(&text, &words, &words).unwrap();
        assert_eq!(split.head_text, "Elle savait bien alors qu' il");
        let curly: Vec<String> = words.iter().map(|w| w.replace('\'', "’")).collect();
        assert_eq!(split_sentence(&curly.join(" "), &curly, &curly).unwrap().head_words.len(), 6);
    }

    #[test]
    fn the_word_cut_never_falls_after_a_number_in_digits() {
        // La forme parlée dit « dix-huit » : c'est l'écrit « 18 » qui compte.
        let s = real_sentence("Le train partira vers 18 heures depuis la gare du centre-ville.");
        assert!(s.spoken_words().contains(&"dix-huit".to_string()), "l'essai suppose « 18 » verbalisé");
        let split = split_sentence(&s.text, &s.spoken_words(), &written(&s)).unwrap();
        assert_eq!(split.head_text, "Le train partira vers dix-huit heures");
        assert_eq!(split.tail_text, "depuis la gare du centre-ville.");
        assert_eq!(split.head_words.len(), 6);
        // « 2027 », puis deux mots-outils : rien d'acceptable avant le 8e mot.
        assert!(split_real("Il est né en 2027 dans une petite ville du nord de la France.").is_none());
    }

    #[test]
    fn a_head_never_ends_on_a_function_word() {
        let split = split_real("Nous partirons demain matin vers la mer avec toute la famille.").unwrap();
        assert_eq!(split.head_text, "Nous partirons demain matin vers la mer");
        let split = split_real("We will take the train to the old city by the river tonight.").unwrap();
        assert_eq!(split.head_text, "We will take the train");
    }

    #[test]
    fn a_comma_still_wins_over_the_word_cut() {
        let split = split_real("Le quatorze juillet, Camille harangua la foule venue de toute la ville.").unwrap();
        assert_eq!(split.head_text, "Le quatorze juillet,");
    }

    #[test]
    fn a_number_said_in_several_words_is_found_in_the_spoken_text() {
        // Le premier mot de la suite se dit « deux mille vingt-sept ».
        let split = split_real("Nous avons reçu hier soir 2027 lettres de toute la région.").unwrap();
        assert_eq!(split.head_text, "Nous avons reçu hier soir");
        assert!(split.tail_text.starts_with("deux mille vingt-sept lettres"), "{}", split.tail_text);
        // « 10 000 » se dit « dix mille » en contexte, « dix » + « zéro zéro
        // zéro » mot à mot : la coupe après « euros » se retrouve quand même.
        let split = split_real("Le seuil fixé était 10 000 euros pour tous les gens du coin.").unwrap();
        assert_eq!(split.head_text, "Le seuil fixé était dix mille euros");
        assert_eq!(split.tail_text, "pour tous les gens du coin.");
    }

    #[test]
    fn a_cut_missing_from_the_spoken_text_tries_the_next_word() {
        let words: Vec<String> =
            "Il a vu hier soir trois chats noirs dans la cour.".split_whitespace().map(str::to_string).collect();
        let split = split_sentence("Il a vu hier soir-là trois chats noirs dans la cour.", &words, &words).unwrap();
        assert_eq!(split.head_text, "Il a vu hier soir-là trois");
        assert_eq!(split.head_words.len(), 6);
    }

    #[test]
    fn a_spoken_text_that_diverges_at_every_word_cut_stays_whole() {
        let s = real_sentence("Il marchait très longtemps seul sans jamais se retourner vraiment.");
        assert!(split_sentence("Il marchait sans cesse ni repos.", &s.spoken_words(), &written(&s)).is_none());
        // Mot entier, pas une fin de mot : « elle » ne vaut pas « le ».
        let words: Vec<String> = "Camille traversa la grande place sous la pluie fine du matin."
            .split_whitespace()
            .map(str::to_string)
            .collect();
        assert!(split_sentence("Camille traversa la grande replace resous lala pluie fine du matin.", &words, &words)
            .is_none());
    }

    /// Minute ses mots (100 ms chacun) et coupe la phrase attendue. Avec
    /// `fail_tail`, refuse toute suite (un texte sans virgule) la première
    /// fois.
    struct SplittingEngine {
        texts: Arc<std::sync::Mutex<Vec<String>>>,
        fail_tail: Arc<AtomicBool>,
        /// Ce que le moteur répond à `splits_first_clause`, à tout moment.
        splits: Arc<AtomicBool>,
    }
    struct SplittingSession {
        texts: Arc<std::sync::Mutex<Vec<String>>>,
        fail_tail: Arc<AtomicBool>,
    }

    impl SplittingEngine {
        fn new(texts: &Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self {
                texts: Arc::clone(texts),
                fail_tail: Arc::new(AtomicBool::new(false)),
                splits: Arc::new(AtomicBool::new(true)),
            }
        }
    }

    impl TtsEngine for SplittingEngine {
        fn name(&self) -> String {
            "coupe".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(SplittingSession {
                texts: Arc::clone(&self.texts),
                fail_tail: Arc::clone(&self.fail_tail),
            }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn splits_first_clause(&self) -> bool {
            self.splits.load(Ordering::SeqCst)
        }
    }

    impl TtsSession for SplittingSession {
        fn synthesize(&mut self, text: &str, _: &TtsParams) -> Result<PcmBuffer> {
            self.texts.locked().push(text.to_string());
            if !text.contains(',') && self.fail_tail.swap(false, Ordering::SeqCst) {
                anyhow::bail!("suite refusée");
            }
            let n = text.split_whitespace().count().max(1);
            Ok(PcmBuffer::new(vec![0.1; n * 100], 1000, 1))
        }
        fn synthesize_timed(&mut self, text: &str, words: &[String], p: &TtsParams) -> Result<Synthesis> {
            let pcm = self.synthesize(text, p)?;
            let spans = (0..words.len())
                .map(|i| WordSpan { start: i as f32 * 0.1, end: (i + 1) as f32 * 0.1 })
                .collect();
            Ok(Synthesis { pcm, words: Some(spans) })
        }
    }

    fn wait_tail(synth: &Synthesizer, id: usize) -> Rendered {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = synth.tail(id) {
                return r;
            }
            assert!(std::time::Instant::now() < deadline, "suite {id} jamais prête");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn only_the_awaited_sentence_is_cut_in_two() {
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = Arc::new(SplittingEngine::new(&texts));
        let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        let second = "Paris bascula, en quelques heures, sans un mot.";
        synth.push_with_words(first.into(), words(first), 300);
        synth.push_with_words(second.into(), words(second), 0);

        let head = wait_ready(&synth, 0);
        assert_eq!(head.part, Part::Head { words: 3 });
        assert_eq!(head.words.as_ref().map(|w| w.len()), Some(3));
        let tail = wait_tail(&synth, 0);
        assert_eq!(tail.part, Part::Whole);
        assert_eq!(tail.words.as_ref().map(|w| w.len()), Some(4));
        assert!(tail.audio.duration > tail.speech, "la pause de fin de bloc va à la suite");
        assert_eq!(wait_ready(&synth, 1).part, Part::Whole, "les phrases suivantes restent entières");
        assert_eq!(texts.locked()[..2], ["Le quatorze juillet,", "Camille harangua la foule."]);
    }

    #[test]
    fn the_cut_keeps_the_spoken_form_of_the_whole_sentence() {
        // « 10 000 » ne se dit bien qu'en contexte : mot à mot, « 000 »
        // deviendrait « zéro ». La tête et la suite sont prises dans la forme
        // parlée de la phrase entière, coupée à la même virgule.
        let mut builder = crate::doc::DocumentBuilder::new(crate::doc::builder::CodePolicy::Announce);
        builder.push_block("Le seuil fixé à 10 000 euros, reste bas pour tous.");
        let sentence = builder.document().sentences[0].clone();
        assert_ne!(
            sentence.spoken_words().join(" "),
            sentence.text,
            "l'essai suppose une forme parlée qui dépend du contexte"
        );
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let synth = Synthesizer::new(Arc::new(SplittingEngine::new(&texts)), TtsParams::default(), 1, 4);
        synth.push_with_words(sentence.text.clone(), sentence.spoken_words(), 0);
        let head = wait_ready(&synth, 0);
        assert_eq!(head.part, Part::Head { words: 7 });
        wait_tail(&synth, 0);
        let said = texts.locked().clone();
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].ends_with(','), "{said:?}");
        assert_eq!(format!("{} {}", said[0], said[1]), sentence.text);
    }

    #[test]
    fn a_comma_said_inside_a_number_does_not_cut_it() {
        // Un numéro se dicte par groupes, séparés à l'oral par des virgules
        // que les mots, eux, n'ont pas : la coupe tomberait en plein numéro.
        let mut builder = crate::doc::DocumentBuilder::new(crate::doc::builder::CodePolicy::Announce);
        builder.push_block("Appelez le 06 12 34 56 78, puis attendez la tonalité.");
        let sentence = builder.document().sentences[0].clone();
        assert!(sentence.text.contains("six, douze,"), "l'essai suppose des groupes dictés : {}", sentence.text);
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let synth = Synthesizer::new(Arc::new(SplittingEngine::new(&texts)), TtsParams::default(), 1, 4);
        synth.push_with_words(sentence.text.clone(), sentence.spoken_words(), 0);
        let rendered = wait_ready(&synth, 0);
        assert_eq!(rendered.part, Part::Whole, "la phrase reste entière");
        assert_eq!(*texts.locked(), std::slice::from_ref(&sentence.text));
        assert_eq!(rendered.words.map(|w| w.len()), Some(sentence.words.len()));
    }

    #[test]
    fn a_spoken_text_without_the_cut_stays_whole() {
        let words = words("Le quatorze juillet, Camille harangua la foule.");
        assert!(split_sentence("Le quatorze juillet Camille harangua la foule.", &words, &words).is_none());
        let split = split_sentence("Le 14 juillet, Camille harangua la foule.", &words, &words).unwrap();
        assert_eq!((split.head_text.as_str(), split.tail_text.as_str()), ("Le 14 juillet,", "Camille harangua la foule."));
        assert_eq!(split.head_words.len(), 3);
    }

    #[test]
    fn the_awaited_sentence_without_a_comma_starts_with_five_words() {
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let synth = Synthesizer::new(Arc::new(SplittingEngine::new(&texts)), TtsParams::default(), 1, 4);
        let first = "Camille traversa la grande place sous la pluie fine du matin.";
        synth.push_with_words(first.into(), words(first), 0);
        assert_eq!(wait_ready(&synth, 0).part, Part::Head { words: 5 });
        assert_eq!(wait_tail(&synth, 0).words.as_ref().map(|w| w.len()), Some(6));
        assert_eq!(texts.locked()[..2], ["Camille traversa la grande place", "sous la pluie fine du matin."]);
    }

    #[test]
    fn the_cut_follows_the_engine_once_it_is_loaded() {
        // Un moteur chargé en différé ne sait qu'après coup s'il coupe : le
        // pool le lui demande à chaque phrase, pas une fois à sa création.
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = Arc::new(SplittingEngine::new(&texts));
        engine.splits.store(false, Ordering::SeqCst);
        let synth = Synthesizer::new(Arc::clone(&engine) as Arc<dyn TtsEngine>, TtsParams::default(), 1, 4);
        engine.splits.store(true, Ordering::SeqCst);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        synth.push_with_words(first.into(), words(first), 0);
        assert_eq!(wait_ready(&synth, 0).part, Part::Head { words: 3 });
    }

    #[test]
    fn a_refused_tail_lets_the_sentence_be_retried_whole() {
        // La tête seule en cache, sans suite ni travail en cours, bloquerait
        // la phrase pour de bon : y revenir doit la refaire.
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let engine = Arc::new(SplittingEngine::new(&texts));
        engine.fail_tail.store(true, Ordering::SeqCst);
        let synth = Synthesizer::new(Arc::clone(&engine) as Arc<dyn TtsEngine>, TtsParams::default(), 1, 4);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        synth.push_with_words(first.into(), words(first), 0);
        assert!(wait_for(|| synth.failure(0).is_some()), "la suite refusée se dit");
        assert!(synth.get(0).is_none(), "la tête orpheline ne reste pas en cache");

        synth.retry_from(0);
        assert_eq!(wait_ready(&synth, 0).part, Part::Head { words: 3 });
        assert_eq!(wait_tail(&synth, 0).words.as_ref().map(|w| w.len()), Some(4));
    }

    #[test]
    fn the_cut_fades_only_its_own_edge() {
        let pcm = || PcmBuffer::new(vec![0.5; 2000], 1000, 1);
        let head = fade_edge(pcm(), Edge::End);
        assert_eq!(head.samples.len(), 2000);
        assert!(head.samples[1999].abs() < 0.01, "{}", head.samples[1999]);
        assert_eq!(head.samples[..1995], [0.5; 1995][..], "5 ms à 1 kHz : le reste intact");
        let tail = fade_edge(pcm(), Edge::Start);
        assert!(tail.samples[0].abs() < 0.01);
        assert_eq!(tail.samples[5..], [0.5; 1995][..]);
        assert_eq!(tail.duration, Duration::from_secs(2));
    }

    #[test]
    fn head_and_tail_reach_the_cache_faded_at_the_cut() {
        let texts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let synth = Synthesizer::new(Arc::new(SplittingEngine::new(&texts)), TtsParams::default(), 1, 4);
        let first = "Le quatorze juillet, Camille harangua la foule.";
        synth.push_with_words(first.into(), words(first), 0);
        let head = wait_ready(&synth, 0);
        let tail = wait_tail(&synth, 0);
        assert!(head.audio.samples.last().unwrap().abs() < 0.01);
        assert_eq!(head.audio.samples[0], 0.1, "le début de la tête n'est pas touché");
        assert!(tail.audio.samples[0].abs() < 0.01);
        assert_eq!(*tail.audio.samples.last().unwrap(), 0.1, "la fin de la suite non plus");
    }

    /// Moteur dont chaque synthèse annonce son texte, attend un feu vert
    /// (le passage MLX jusqu'aux durées), puis consulte le test d'abandon :
    /// abandonnée, elle le note et rend `Cancelled`.
    struct Abandoning {
        started: std::sync::Mutex<std::sync::mpsc::Sender<String>>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
        abandoned: Arc<std::sync::Mutex<Vec<String>>>,
        splits: bool,
    }
    struct AbandoningSession {
        started: std::sync::mpsc::Sender<String>,
        gate: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
        abandoned: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TtsEngine for Abandoning {
        fn name(&self) -> String {
            "abandon".into()
        }
        fn open_session(&self) -> Result<Box<dyn TtsSession>> {
            Ok(Box::new(AbandoningSession {
                started: self.started.locked().clone(),
                gate: Arc::clone(&self.gate),
                abandoned: Arc::clone(&self.abandoned),
            }))
        }
        fn max_sessions(&self) -> usize {
            1
        }
        fn splits_first_clause(&self) -> bool {
            self.splits
        }
    }

    impl TtsSession for AbandoningSession {
        fn synthesize(&mut self, _: &str, _: &TtsParams) -> Result<PcmBuffer> {
            unreachable!("le pool passe par synthesize_cancellable")
        }
        fn synthesize_cancellable(
            &mut self,
            text: &str,
            _: &[String],
            _: &TtsParams,
            cancel: &crate::tts::Cancel,
        ) -> Result<Synthesis> {
            self.started.send(text.to_string()).unwrap();
            // Banc démonté (feu vert lâché) : la synthèse finit tout de suite.
            let _ = self.gate.locked().recv_timeout(Duration::from_secs(5));
            if cancel.is_cancelled() {
                self.abandoned.locked().push(text.to_string());
                return Err(crate::tts::Cancelled.into());
            }
            Ok(Synthesis { pcm: PcmBuffer::new(vec![0.1; text.len() * 10], 1000, 1), words: None })
        }
    }

    /// `release` avant `synth` : lâché d'abord, il libère le worker en
    /// attente et la fermeture du pool ne l'attend pas.
    struct AbandonRig {
        release: std::sync::mpsc::Sender<()>,
        synth: Synthesizer,
        started: std::sync::mpsc::Receiver<String>,
        abandoned: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl AbandonRig {
        fn new(splits: bool, sentences: &[String]) -> Self {
            let (started_tx, started) = std::sync::mpsc::channel();
            let (release, gate) = std::sync::mpsc::channel();
            let abandoned = Arc::new(std::sync::Mutex::new(Vec::new()));
            let engine = Arc::new(Abandoning {
                started: std::sync::Mutex::new(started_tx),
                gate: Arc::new(std::sync::Mutex::new(gate)),
                abandoned: Arc::clone(&abandoned),
                splits,
            });
            let synth = Synthesizer::new(engine, TtsParams::default(), 1, 4);
            for s in sentences {
                synth.push_with_words(s.clone(), words(s), 0);
            }
            Self { synth, started, release, abandoned }
        }
        fn next_started(&self) -> String {
            self.started.recv_timeout(Duration::from_secs(5)).expect("une synthèse démarre")
        }
        fn abandoned(&self) -> Vec<String> {
            self.abandoned.locked().clone()
        }
    }

    fn numbered(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("phrase {i}")).collect()
    }

    #[test]
    fn a_jump_away_abandons_the_synthesis_in_flight() {
        let rig = AbandonRig::new(false, &numbered(20));
        assert_eq!(rig.next_started(), "phrase 0");
        rig.synth.set_cursor(10);
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "phrase 10", "la nouvelle phrase part aussitôt après");
        assert_eq!(rig.abandoned(), ["phrase 0"]);
        assert!(rig.synth.get(0).is_none(), "rien en cache");
        assert_eq!(rig.synth.failure(0), None, "un abandon n'est pas un échec");
        assert_eq!(rig.synth.stats().failed, 0);
        rig.release.send(()).unwrap();
        assert_eq!(wait_ready(&rig.synth, 10).audio.samples.len(), "phrase 10".len() * 10);

        // La phrase abandonnée reste disponible : y revenir la refait (et
        // abandonne la phrase d'avance prise entre-temps).
        assert_eq!(rig.next_started(), "phrase 11");
        rig.synth.set_cursor(0);
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "phrase 0");
        rig.release.send(()).unwrap();
        wait_ready(&rig.synth, 0);
        assert_eq!(rig.abandoned(), ["phrase 0", "phrase 11"]);
    }

    #[test]
    fn a_new_generation_abandons_the_synthesis_in_flight() {
        let rig = AbandonRig::new(false, &numbered(6));
        assert_eq!(rig.next_started(), "phrase 0");
        rig.synth.set_params(TtsParams { voice: "autre".into(), ..TtsParams::default() });
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "phrase 0", "refaite sous les nouveaux paramètres");
        assert_eq!(rig.abandoned(), ["phrase 0"]);
        assert_eq!(rig.synth.failure(0), None);
        rig.release.send(()).unwrap();
        wait_ready(&rig.synth, 0);

        // Un nouveau texte aussi.
        assert_eq!(rig.next_started(), "phrase 1");
        rig.synth.reset();
        rig.synth.push("bb".into(), 0);
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "bb");
        assert_eq!(rig.abandoned(), ["phrase 0", "phrase 1"]);
        rig.release.send(()).unwrap();
        wait_ready(&rig.synth, 0);
    }

    #[test]
    fn a_sentence_still_wanted_is_never_abandoned() {
        let rig = AbandonRig::new(false, &numbered(8));
        assert_eq!(rig.next_started(), "phrase 0");
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "phrase 1");
        // Le curseur avance sur la phrase en cours : toujours utile.
        rig.synth.set_cursor(1);
        rig.release.send(()).unwrap();
        wait_ready(&rig.synth, 1);
        assert_eq!(rig.next_started(), "phrase 2");
        // Elle reste dans la fenêtre [curseur, curseur + 4).
        rig.synth.set_cursor(2);
        rig.release.send(()).unwrap();
        wait_ready(&rig.synth, 2);
        assert!(rig.abandoned().is_empty(), "{:?}", rig.abandoned());
        assert_eq!(rig.synth.stats().failed, 0);
    }

    #[test]
    fn the_tail_of_a_sentence_left_behind_is_abandoned() {
        // Un petit saut (0 → 1) : la phrase 0 reste dans `keep_behind`, rien
        // ne l'évince ; seule l'abandon peut retirer sa tête orpheline.
        let mut sentences = vec!["Le quatorze juillet, Camille harangua la foule.".to_string()];
        sentences.extend(numbered(19).into_iter().skip(1));
        let rig = AbandonRig::new(true, &sentences);
        assert_eq!(rig.next_started(), "Le quatorze juillet,");
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "Camille harangua la foule.");
        wait_ready(&rig.synth, 0);
        rig.synth.set_cursor(1);
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "phrase 1");
        assert_eq!(rig.abandoned(), ["Camille harangua la foule."]);
        assert!(rig.synth.get(0).is_none(), "la tête orpheline ne reste pas en cache");
        assert!(rig.synth.tail(0).is_none());
        assert_eq!(rig.synth.failure(0), None);

        // Le curseur revient : la phrase est refaite entière, tête et suite,
        // au lieu d'attendre pour toujours une suite qui ne viendra pas.
        rig.synth.set_cursor(0);
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "Le quatorze juillet,");
        rig.release.send(()).unwrap();
        assert_eq!(rig.next_started(), "Camille harangua la foule.");
        rig.release.send(()).unwrap();
        assert_eq!(wait_tail(&rig.synth, 0).part, Part::Whole);
        assert_eq!(rig.synth.get(0).map(|r| r.part), Some(Part::Head { words: 3 }));
    }

    #[test]
    fn trailing_silence_extends_audio_but_not_speech() {
        let pcm = PcmBuffer::new(vec![0.5; 1000], 1000, 1);
        let rendered = with_trailing_silence(Synthesis { pcm, words: None }, 0, 500);
        assert_eq!(rendered.speech, Duration::from_millis(1000));
        assert_eq!(rendered.audio.duration, Duration::from_millis(1500));
        assert_eq!(rendered.audio.samples.len(), 1500);
    }
}

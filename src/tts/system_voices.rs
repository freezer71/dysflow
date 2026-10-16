//! Les voix que `say` sait prendre : celles installées sur ce Mac.
//!
//! L'app ne peut pas en télécharger d'autres (aucune API) : l'utilisateur
//! les ajoute dans Réglages Système > Accessibilité > Contenu énoncé.

use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemVoice {
    pub name: String,
    /// `fr_FR`, `en_US`…
    pub locale: String,
}

impl SystemVoice {
    /// `fr` ou `en`.
    pub fn language(&self) -> &str {
        self.locale.split('_').next().unwrap_or("")
    }
}

/// Lit la sortie de `say -v '?'` : « Nom (peut avoir des espaces) locale # phrase ».
/// Garde le français et l'anglais, dans l'ordre de `say`.
pub fn parse_say_voices(listing: &str) -> Vec<SystemVoice> {
    listing
        .lines()
        .filter_map(|line| {
            let head = line.split('#').next()?.trim_end();
            let (name, locale) = head.rsplit_once(char::is_whitespace)?;
            Some(SystemVoice { name: name.trim().to_string(), locale: locale.to_string() })
        })
        .filter(|v| matches!(v.language(), "fr" | "en"))
        .collect()
}

/// Une relecture plus rapprochée que ceci reprend la liste déjà lue : rouvrir
/// les réglages plusieurs fois de suite ne relance pas `say` à chaque fois.
const FRESH: Duration = Duration::from_secs(2);

/// La liste lue, et quand.
#[derive(Default)]
struct Cache {
    read_at: Option<Instant>,
    voices: Vec<SystemVoice>,
}

impl Cache {
    /// La liste, lue par `read` si elle ne l'a jamais été.
    fn get(&mut self, now: Instant, read: impl FnOnce() -> Vec<SystemVoice>) -> Vec<SystemVoice> {
        if self.read_at.is_none() {
            self.store(now, read());
        }
        self.voices.clone()
    }

    /// Relit la liste, sauf lue il y a moins de `FRESH`.
    fn refresh(&mut self, now: Instant, read: impl FnOnce() -> Vec<SystemVoice>) {
        if self.read_at.is_none_or(|at| now.saturating_duration_since(at) >= FRESH) {
            self.store(now, read());
        }
    }

    fn store(&mut self, now: Instant, voices: Vec<SystemVoice>) {
        self.read_at = Some(now);
        self.voices = voices;
    }
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { read_at: None, voices: Vec::new() });

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Lance `say -v '?'`. Vide hors macOS ou si `say` manque.
fn read_say() -> Vec<SystemVoice> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    Command::new("say")
        .args(["-v", "?"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_say_voices(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Les voix de ce Mac, telles que lues la dernière fois (lues ici si elles
/// ne l'ont jamais été).
pub fn system_voices() -> Vec<SystemVoice> {
    cache().get(Instant::now(), read_say)
}

/// Relit les voix de ce Mac : une voix ajoutée dans Réglages Système depuis
/// la dernière lecture apparaît. Appelée quand les réglages rechargent les
/// capacités, hors du fil du lecteur (`say` prend quelques dizaines de ms).
pub fn refresh_system_voices() {
    // `say` tourne hors du verrou : un `system_voices()` concurrent n'attend pas.
    let stale = cache().read_at.is_none_or(|at| at.elapsed() >= FRESH);
    if stale {
        let voices = read_say();
        cache().refresh(Instant::now(), || voices);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = "\
Amélie              fr_CA    # Bonjour, je m’appelle Amélie.
Eddy (French (France)) fr_FR    # Bonjour, je m’appelle Eddy.
Samantha            en_US    # Hello! My name is Samantha.
Kyoko               ja_JP    # こんにちは、私の名前はKyokoです。
";

    #[test]
    fn names_may_hold_spaces_and_parentheses() {
        let voices = parse_say_voices(LISTING);
        assert_eq!(voices[1], SystemVoice { name: "Eddy (French (France))".into(), locale: "fr_FR".into() });
    }

    #[test]
    fn only_french_and_english_voices_are_kept() {
        let names: Vec<String> = parse_say_voices(LISTING).into_iter().map(|v| v.name).collect();
        assert_eq!(names, ["Amélie", "Eddy (French (France))", "Samantha"]);
    }

    fn voice(name: &str) -> SystemVoice {
        SystemVoice { name: name.into(), locale: "fr_FR".into() }
    }

    #[test]
    fn the_list_is_read_once_then_kept() {
        let mut cache = Cache::default();
        let t0 = Instant::now();
        assert_eq!(cache.get(t0, || vec![voice("Thomas")]), [voice("Thomas")]);
        assert_eq!(cache.get(t0 + Duration::from_secs(60), || panic!("pas relue")), [voice("Thomas")]);
    }

    /// Une voix ajoutée dans Réglages Système apparaît à la relecture que
    /// font les réglages, sans relancer l'app.
    #[test]
    fn a_refresh_shows_a_voice_added_since() {
        let mut cache = Cache::default();
        let t0 = Instant::now();
        cache.get(t0, || vec![voice("Thomas")]);
        cache.refresh(t0 + FRESH, || vec![voice("Thomas"), voice("Audrey")]);
        assert_eq!(cache.get(t0 + FRESH, || panic!("pas relue")), [voice("Thomas"), voice("Audrey")]);
    }

    #[test]
    fn a_refresh_right_after_a_read_keeps_the_list() {
        let mut cache = Cache::default();
        let t0 = Instant::now();
        cache.get(t0, || vec![voice("Thomas")]);
        cache.refresh(t0 + Duration::from_millis(500), || panic!("lue il y a moins de FRESH"));
        assert_eq!(cache.get(t0, || panic!("pas relue")), [voice("Thomas")]);
    }

    #[test]
    fn blank_lines_and_lines_without_hash_do_not_panic() {
        let voices = parse_say_voices("\n   \nseul\nThomas fr_FR\n");
        assert_eq!(voices, [SystemVoice { name: "Thomas".into(), locale: "fr_FR".into() }]);
    }
}

//! Langue de l'interface (anglais ou français) du moteur et du terminal.
//!
//! Seule l'interface est traduite : jamais le texte lu, les phonèmes ni la
//! détection de la langue des documents. Règle commune : français si le code de
//! langue de l'étiquette (ce qui précède le premier `_`, `-`, `.` ou `@`) vaut
//! `fr`, sans tenir compte de la casse ; anglais sinon.
//!
//! `ui_lang()` répond dans cet ordre : la surcharge du fil (`with_ui_lang`),
//! puis la valeur globale (`set_ui_lang`), puis le défaut. Le défaut est `En`,
//! sauf dans les tests unitaires de cette crate (`cfg(test)`) où il vaut `Fr` :
//! les assertions françaises existantes y restent la couverture du français, et
//! chaque test anglais passe par `with_ui_lang`. Les tests d'intégration et la
//! crate `ffi` lient la bibliothèque hors `cfg(test)` : leur défaut est `En`.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLang {
    En,
    Fr,
}

impl UiLang {
    /// Langue d'une étiquette de locale (`fr_FR.UTF-8`, `fr-CA`, `C`…).
    pub fn from_locale(tag: &str) -> UiLang {
        let code = tag.split(['_', '-', '.', '@']).next().unwrap_or("");
        if code.eq_ignore_ascii_case("fr") {
            UiLang::Fr
        } else {
            UiLang::En
        }
    }

    /// Langue de l'environnement : `LC_ALL`, puis `LC_MESSAGES`, puis `LANG` ;
    /// une variable vide compte comme absente.
    pub fn from_env() -> UiLang {
        UiLang::from_env_with(|k| std::env::var(k).ok())
    }

    /// Comme `from_env`, avec une lecture de variables injectable.
    pub fn from_env_with(get: impl Fn(&str) -> Option<String>) -> UiLang {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| get(k))
            .find(|v| !v.is_empty())
            .map_or(UiLang::En, |v| UiLang::from_locale(&v))
    }

    pub fn code(self) -> &'static str {
        match self {
            UiLang::En => "en",
            UiLang::Fr => "fr",
        }
    }
}

// 0 = jamais fixée (défaut), 1 = anglais, 2 = français.
static GLOBAL: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<UiLang>> = const { Cell::new(None) };
}

#[cfg(test)]
const DEFAULT: UiLang = UiLang::Fr;
#[cfg(not(test))]
const DEFAULT: UiLang = UiLang::En;

/// Fixe la langue globale de l'interface.
pub fn set_ui_lang(lang: UiLang) {
    let v = match lang {
        UiLang::En => 1,
        UiLang::Fr => 2,
    };
    GLOBAL.store(v, Ordering::Relaxed);
}

/// Langue courante : surcharge du fil, sinon globale, sinon défaut.
pub fn ui_lang() -> UiLang {
    if let Some(l) = OVERRIDE.with(Cell::get) {
        return l;
    }
    match GLOBAL.load(Ordering::Relaxed) {
        1 => UiLang::En,
        2 => UiLang::Fr,
        _ => DEFAULT,
    }
}

/// Exécute `f` avec une langue propre au fil courant (pour les tests). La
/// surcharge précédente est rétablie même si `f` panique.
pub fn with_ui_lang<R>(lang: UiLang, f: impl FnOnce() -> R) -> R {
    struct Restaure(Option<UiLang>);
    impl Drop for Restaure {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _garde = Restaure(OVERRIDE.with(|o| o.replace(Some(lang))));
    f()
}

/// Un texte d'interface dans les deux langues ; utilisable en `const`.
#[derive(Clone, Copy, Debug)]
pub struct Tr {
    pub en: &'static str,
    pub fr: &'static str,
}

impl Tr {
    pub const fn new(en: &'static str, fr: &'static str) -> Tr {
        Tr { en, fr }
    }

    pub fn get(self) -> &'static str {
        match ui_lang() {
            UiLang::En => self.en,
            UiLang::Fr => self.fr,
        }
    }
}

/// `tr!("english", "français")` donne un `&'static str` ;
/// `tr!(en = "{} voices", fr = "{} voix", n)` donne un `String` (`format!` sur
/// le gabarit de la langue courante ; les identifiants `{x}` fonctionnent aussi).
#[macro_export]
macro_rules! tr {
    ($en:literal, $fr:literal $(,)?) => {
        match $crate::i18n::ui_lang() {
            $crate::i18n::UiLang::En => $en,
            $crate::i18n::UiLang::Fr => $fr,
        }
    };
    (en = $en:literal, fr = $fr:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::ui_lang() {
            $crate::i18n::UiLang::En => format!($en $(, $arg)*),
            $crate::i18n::UiLang::Fr => format!($fr $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquettes() {
        for fr in ["fr", "fr_FR.UTF-8", "fr-CA", "fr_BE", "FR_fr"] {
            assert_eq!(UiLang::from_locale(fr), UiLang::Fr, "{fr}");
        }
        for en in ["en_US.UTF-8", "de_DE", "C", "POSIX", "", "francais"] {
            assert_eq!(UiLang::from_locale(en), UiLang::En, "{en}");
        }
    }

    #[test]
    fn ordre_des_variables() {
        // from_env lit par une fonction injectable : from_env_with(|k| …)
        let env = |vars: &[(&str, &str)]| {
            let v: Vec<_> = vars
                .iter()
                .map(|(k, x)| (k.to_string(), x.to_string()))
                .collect();
            move |k: &str| v.iter().find(|(n, _)| n == k).map(|(_, x)| x.clone())
        };
        assert_eq!(
            UiLang::from_env_with(env(&[("LANG", "fr_FR.UTF-8")])),
            UiLang::Fr
        );
        assert_eq!(
            UiLang::from_env_with(env(&[("LC_ALL", "en_US.UTF-8"), ("LANG", "fr_FR.UTF-8")])),
            UiLang::En
        );
        assert_eq!(
            UiLang::from_env_with(env(&[("LC_ALL", ""), ("LANG", "fr_FR.UTF-8")])),
            UiLang::Fr
        );
        assert_eq!(
            UiLang::from_env_with(env(&[("LC_MESSAGES", "fr_CA"), ("LANG", "C")])),
            UiLang::Fr
        );
        assert_eq!(UiLang::from_env_with(env(&[])), UiLang::En);
    }

    #[test]
    fn surcharge_du_fil() {
        assert_eq!(
            with_ui_lang(UiLang::En, || tr!("Settings", "Réglages")),
            "Settings"
        );
        assert_eq!(
            with_ui_lang(UiLang::Fr, || tr!(en = "{} voices", fr = "{} voix", 3)),
            "3 voix"
        );
    }

    #[test]
    fn defaut_de_test_et_restauration() {
        assert_eq!(ui_lang(), UiLang::Fr);
        let r = std::panic::catch_unwind(|| with_ui_lang(UiLang::En, || panic!("boum")));
        assert!(r.is_err());
        assert_eq!(ui_lang(), UiLang::Fr);
        assert_eq!(
            with_ui_lang(UiLang::En, || with_ui_lang(UiLang::Fr, ui_lang)),
            UiLang::Fr
        );
    }

    #[test]
    fn const_et_identifiants() {
        const T: Tr = Tr::new("Save", "Enregistrer");
        assert_eq!(with_ui_lang(UiLang::En, || T.get()), "Save");
        let n = 2;
        assert_eq!(
            with_ui_lang(UiLang::En, || tr!(en = "{n} voices", fr = "{n} voix")),
            "2 voices"
        );
    }
}

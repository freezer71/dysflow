//! Le calcul des modèles, au plus près du matériel de chaque système.

pub mod gpu;
#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
pub mod mlx;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Un moyen de calculer un modèle sur cette machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator {
    Mlx,
    OrtCpu,
}

impl Accelerator {
    /// Le nom accepté par `DYSFLOW_ACCELERATOR`.
    pub fn name(self) -> &'static str {
        match self {
            Accelerator::Mlx => "mlx",
            Accelerator::OrtCpu => "ort-cpu",
        }
    }
}

/// Du meilleur au moins bon. `app_mlx` : l'app fournit le calcul MLX (Swift),
/// tant que les ports du lot 3 et suivants ne sont pas faits. Sans elle, MLX
/// vient du binaire (feature `mlx`, Mac Apple Silicon) : quels moteurs il
/// calcule, `models::Runtime::loadable` le dit.
pub fn available(app_mlx: bool) -> Vec<Accelerator> {
    let mut found = Vec::new();
    if app_mlx || cfg!(all(feature = "mlx", target_os = "macos", target_arch = "aarch64")) {
        found.push(Accelerator::Mlx);
    }
    if cfg!(feature = "kokoro-onnx") {
        found.push(Accelerator::OrtCpu);
    }
    found
}

/// `DYSFLOW_ACCELERATOR` = "mlx" | "ort-cpu" : met cet accélérateur en tête
/// s'il est dans `found` ; sinon `found` inchangé et `Some(avertissement)`.
/// L'appelant lit la variable une fois, au démarrage.
pub fn forced(found: Vec<Accelerator>, value: Option<&str>) -> (Vec<Accelerator>, Option<String>) {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return (found, None);
    };
    let wanted = [Accelerator::Mlx, Accelerator::OrtCpu].into_iter().find(|a| a.name() == value);
    match wanted.filter(|a| found.contains(a)) {
        Some(a) => {
            let mut order = vec![a];
            order.extend(found.into_iter().filter(|x| *x != a));
            (order, None)
        }
        None => {
            let warning = crate::tr!(
                en = "DYSFLOW_ACCELERATOR=\"{}\" ignored: not available on this machine",
                fr = "DYSFLOW_ACCELERATOR=\"{}\" ignoré : indisponible sur cette machine",
                value,
            );
            (found, Some(warning))
        }
    }
}

/// `available` + `DYSFLOW_ACCELERATOR`, lu une fois par processus.
/// L'avertissement d'un accélérateur forcé indisponible n'est rendu qu'au
/// premier appel qui le produit : un moteur reconstruit ne le répète pas.
pub fn ordered(app_mlx: bool) -> (Vec<Accelerator>, Option<String>) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    ordered_with(available(app_mlx), forced_value(), &WARNED)
}

/// L'accélérateur que `DYSFLOW_ACCELERATOR` force, s'il est dans
/// `available(app_mlx)` : il fait passer un paquet installé de cet
/// accélérateur avant le choix enregistré (`models::forced_choice`).
pub fn forced_accelerator(app_mlx: bool) -> Option<Accelerator> {
    let value = forced_value()?.trim();
    available(app_mlx).into_iter().find(|a| a.name() == value)
}

/// `DYSFLOW_ACCELERATOR`, lu une fois par processus.
fn forced_value() -> Option<&'static str> {
    static VALUE: OnceLock<Option<String>> = OnceLock::new();
    VALUE.get_or_init(|| std::env::var("DYSFLOW_ACCELERATOR").ok()).as_deref()
}

/// `forced`, dont l'avertissement ne sort qu'une fois par `warned`.
fn ordered_with(found: Vec<Accelerator>, value: Option<&str>, warned: &AtomicBool) -> (Vec<Accelerator>, Option<String>) {
    let (order, warning) = forced(found, value);
    (order, warning.filter(|_| !warned.swap(true, Ordering::SeqCst)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_on_mac_prefers_mlx() {
        assert_eq!(available(true).first(), Some(&Accelerator::Mlx));
    }

    #[test]
    fn without_the_app_mlx_comes_only_from_the_rust_build() {
        let found = available(false);
        assert_eq!(
            found.contains(&Accelerator::Mlx),
            cfg!(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))
        );
        assert_eq!(found.contains(&Accelerator::OrtCpu), cfg!(feature = "kokoro-onnx"));
    }

    #[test]
    fn an_unavailable_forced_accelerator_is_ignored_with_a_warning() {
        let (order, warning) = forced(vec![Accelerator::OrtCpu], Some("mlx"));
        assert_eq!(order, vec![Accelerator::OrtCpu]);
        assert!(warning.is_some_and(|w| w.contains("mlx")));
    }

    #[test]
    fn an_unknown_forced_value_is_ignored_with_a_warning() {
        let (order, warning) = forced(vec![Accelerator::Mlx, Accelerator::OrtCpu], Some("cuda"));
        assert_eq!(order, vec![Accelerator::Mlx, Accelerator::OrtCpu]);
        assert!(warning.is_some_and(|w| w.contains("cuda")));
    }

    #[test]
    fn a_forced_available_accelerator_comes_first() {
        let (order, warning) = forced(vec![Accelerator::Mlx, Accelerator::OrtCpu], Some("ort-cpu"));
        assert_eq!(order, vec![Accelerator::OrtCpu, Accelerator::Mlx]);
        assert!(warning.is_none());
    }

    #[test]
    fn no_forced_value_changes_nothing() {
        let (order, warning) = forced(vec![Accelerator::Mlx], None);
        assert_eq!(order, vec![Accelerator::Mlx]);
        assert!(warning.is_none());
    }

    #[test]
    fn an_unavailable_forced_accelerator_warns_once() {
        let warned = AtomicBool::new(false);
        let (order, first) = ordered_with(vec![Accelerator::OrtCpu], Some("mlx"), &warned);
        assert_eq!(order, vec![Accelerator::OrtCpu]);
        assert!(first.is_some_and(|w| w.contains("mlx")));
        let (order, again) = ordered_with(vec![Accelerator::OrtCpu], Some("mlx"), &warned);
        assert_eq!(order, vec![Accelerator::OrtCpu], "toujours ignoré");
        assert_eq!(again, None, "dit une fois");
    }

    #[test]
    fn an_available_forced_accelerator_says_nothing() {
        let warned = AtomicBool::new(false);
        let (order, warning) = ordered_with(vec![Accelerator::Mlx, Accelerator::OrtCpu], Some("ort-cpu"), &warned);
        assert_eq!(order, vec![Accelerator::OrtCpu, Accelerator::Mlx]);
        assert_eq!(warning, None);
        assert!(!warned.load(Ordering::SeqCst), "rien n'a été dit : une erreur plus tard le serait encore");
    }
}

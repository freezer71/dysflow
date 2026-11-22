//! Le calcul des modèles, au plus près du matériel de chaque système.

pub mod gpu;
#[cfg(all(feature = "mlx", target_os = "macos", target_arch = "aarch64"))]
pub mod mlx;

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
/// tant que les ports du lot 3 et suivants ne sont pas faits.
pub fn available(app_mlx: bool) -> Vec<Accelerator> {
    let mut found = Vec::new();
    // `cfg!(feature = "mlx")` s'ajoutera ici au lot 3 : jusque-là, la feature
    // Rust n'a aucun moteur MLX, seul le calcul Swift de l'app en fournit un.
    if app_mlx {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_on_mac_prefers_mlx() {
        assert_eq!(available(true).first(), Some(&Accelerator::Mlx));
    }

    #[test]
    fn without_mlx_only_ort_cpu_remains_when_onnx_is_built() {
        let found = available(false);
        assert!(!found.contains(&Accelerator::Mlx));
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
}

//! Les plafonds de la copie Rust de MLX, comme `MLXBudget` de l'app pour la
//! sienne. Ils sont globaux au processus : chaque modèle chargé inscrit son
//! besoin, les plafonds valent **la somme** des besoins inscrits, reposée à
//! chaque changement. Rien d'inscrit : ils restent tels quels.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::worker;
use crate::sync::LockExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub memory: usize,
    pub cache: usize,
}

struct Budget {
    needs: Mutex<BTreeMap<String, Need>>,
}

static BUDGET: Budget = Budget::new();

impl Budget {
    const fn new() -> Self {
        Self { needs: Mutex::new(BTreeMap::new()) }
    }

    fn set(&self, key: &str, need: Need) {
        self.needs.locked().insert(key.to_string(), need);
    }

    fn remove(&self, key: &str) {
        self.needs.locked().remove(key);
    }

    fn total(&self) -> Need {
        self.needs.locked().values().fold(Need { memory: 0, cache: 0 }, |sum, need| Need {
            memory: sum.memory.saturating_add(need.memory),
            cache: sum.cache.saturating_add(need.cache),
        })
    }

    /// Les plafonds à poser ; `None` : rien à plafonner.
    fn limits(&self) -> Option<Need> {
        Some(self.total()).filter(|total| total.memory > 0)
    }
}

/// Inscrit (ou remplace) le besoin de `key`, puis repose les plafonds. À
/// appeler avant de charger : tout travail demandé ensuite, ou la suite du
/// travail en cours, se fait sous ces plafonds. N'attend jamais le GPU.
pub fn set(key: &str, need: Need) {
    BUDGET.set(key, need);
    apply();
}

/// Retire le besoin de `key` (inconnue : sans effet), puis repose les
/// plafonds. N'attend jamais le GPU.
pub fn remove(key: &str) {
    BUDGET.remove(key);
    apply();
}

/// La somme des besoins inscrits.
pub fn total() -> Need {
    BUDGET.total()
}

/// Poser un plafond est un appel MLX : sur le fil MLX, jamais pendant un
/// autre calcul.
fn apply() {
    worker::soon(|| {
        if let Some(limits) = BUDGET.limits() {
            mlx_rs::memory::set_memory_limit(limits.memory)?;
            mlx_rs::memory::set_cache_limit(limits.cache)?;
        }
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_are_the_sum_of_the_needs() {
        let budget = Budget::new();
        budget.set("a", Need { memory: 1 << 30, cache: 64 << 20 });
        budget.set("b", Need { memory: 2 << 30, cache: 256 << 20 });
        assert_eq!(budget.total(), Need { memory: 3 << 30, cache: 320 << 20 });
        assert_eq!(budget.limits(), Some(budget.total()));
    }

    #[test]
    fn a_need_is_replaced_by_its_key_and_removed_by_it() {
        let budget = Budget::new();
        let small = Need { memory: 1 << 30, cache: 64 << 20 };
        let big = Need { memory: 4 << 30, cache: 128 << 20 };
        budget.set("a", small);
        budget.set("a", big);
        assert_eq!(budget.total(), big);
        budget.set("b", small);
        budget.remove("a");
        budget.remove("inconnue");
        assert_eq!(budget.total(), small);
    }

    #[test]
    fn nothing_loaded_leaves_the_limits_alone() {
        let budget = Budget::new();
        assert_eq!(budget.limits(), None);
        budget.set("a", Need { memory: 1 << 30, cache: 1 });
        budget.remove("a");
        assert_eq!(budget.limits(), None);
    }

    /// Une somme qui déborde ne doit ni paniquer ni devenir minuscule.
    #[test]
    fn a_huge_sum_saturates() {
        let budget = Budget::new();
        let all = Need { memory: usize::MAX, cache: usize::MAX };
        budget.set("a", all);
        budget.set("b", all);
        assert_eq!(budget.total(), all);
    }
}

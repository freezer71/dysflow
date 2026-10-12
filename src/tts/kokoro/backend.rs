//! Le calcul de Kokoro, derrière un trait : MLX dans l'app (interface
//! étrangère implémentée en Swift), ONNX Runtime au terminal.

use anyhow::{ensure, Result};
use std::path::Path;

/// Ce que le calcul rend : l'audio à 24 kHz, et une durée par jeton
/// d'entrée, en trames de 600 échantillons.
#[derive(Debug, Clone, PartialEq)]
pub struct KokoroRaw {
    pub samples: Vec<f32>,
    pub durations: Vec<u32>,
}

/// Le calcul seul : identifiants et ligne de style en entrée, audio et
/// durées en sortie. Les appels arrivent sur le fil de synthèse et bloquent
/// jusqu'au résultat. `speed` vaut toujours 1 : la vitesse est appliquée au
/// son, par Sonic.
pub trait KokoroBackend: Send + Sync {
    /// « MLX », « int8 » : pour le nom du moteur.
    fn label(&self) -> &'static str;
    fn load(&self, model_dir: &Path) -> Result<()>;
    /// Une courte synthèse, pour compiler ce qui doit l'être.
    fn warm_up(&self) -> Result<()>;
    fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw>;
    /// Libère poids et caches : la voix part au repos.
    fn release(&self);
}

/// Une sortie utilisable : une durée par jeton, aucun NaN, et exactement
/// la somme des durées × 600 échantillons (vérifié au spike).
pub fn check(raw: &KokoroRaw, tokens: usize) -> Result<()> {
    ensure!(raw.durations.len() == tokens, "sortie Kokoro invalide : {} durées pour {tokens} jetons", raw.durations.len());
    ensure!(raw.samples.iter().all(|s| s.is_finite()), "sortie Kokoro invalide : valeur non finie");
    let expected: usize = raw.durations.iter().map(|&d| d as usize).sum::<usize>() * super::FRAME;
    ensure!(
        raw.samples.len() == expected,
        "sortie Kokoro invalide : {} échantillons au lieu de {expected}",
        raw.samples.len()
    );
    Ok(())
}

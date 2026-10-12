//! Le calcul de Kokoro, derrière un trait : MLX dans l'app (interface
//! étrangère implémentée en Swift), ONNX Runtime au terminal.

use anyhow::{ensure, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Au-delà, en moyenne, une durée est absurde : 40 trames (1 s) par
/// phonème. La parole en fait 2 à 6.
const MAX_MEAN_FRAMES: usize = 40;

/// Une sortie utilisable : une durée par jeton, aucun NaN, des durées
/// plausibles, et exactement la somme des durées × 600 échantillons
/// (vérifié au spike).
pub fn check(raw: &KokoroRaw, tokens: usize) -> Result<()> {
    ensure!(raw.durations.len() == tokens, "sortie Kokoro invalide : {} durées pour {tokens} jetons", raw.durations.len());
    ensure!(raw.samples.iter().all(|s| s.is_finite()), "sortie Kokoro invalide : valeur non finie");
    let frames: usize = raw.durations.iter().map(|&d| d as usize).sum();
    ensure!(
        frames <= tokens.max(1) * MAX_MEAN_FRAMES,
        "sortie Kokoro invalide : {frames} trames pour {tokens} jetons"
    );
    let expected = frames * super::FRAME;
    ensure!(
        raw.samples.len() == expected,
        "sortie Kokoro invalide : {} échantillons au lieu de {expected}",
        raw.samples.len()
    );
    Ok(())
}

/// Un calcul partagé entre les moteurs successifs : l'app n'en a qu'un (MLX,
/// sur la puce graphique), et un moteur se reconstruit à chaque changement
/// de voix pendant que l'ancien vit encore (aperçu relâché sur un autre
/// fil, chargement différé orphelin). Sans compte, le `release` de l'ancien
/// viderait les poids que le nouveau vient de charger.
///
/// Chaque `load` réussi prend une part, chaque `release` en rend une ; les
/// poids ne partent qu'avec la dernière. Le verrou sérialise aussi les
/// chargements simultanés. Un `load` d'un autre dossier recharge : la part
/// du calcul sur l'ancien dossier est rendue (`release`) avant, si bien que
/// le calcul voit toujours des paires `load`/`release` — l'app compte les
/// siennes (la préparation des noyaux en est une autre).
///
/// La fabrique de l'app (Tâche 12) doit envelopper le calcul Swift **une
/// fois** dans un `SharedCompute` et passer ce même `Arc` à chaque
/// `build_engine_with` : deux enveloppes autour du même calcul ne se
/// verraient pas.
pub struct SharedCompute {
    inner: Arc<dyn KokoroBackend>,
    /// Parts prises, et le dossier dont les poids sont chargés (aucun si le
    /// calcul ne tient rien).
    state: Mutex<(usize, Option<PathBuf>)>,
}

impl SharedCompute {
    pub fn new(inner: Arc<dyn KokoroBackend>) -> Self {
        Self { inner, state: Mutex::new((0, None)) }
    }
}

impl KokoroBackend for SharedCompute {
    fn label(&self) -> &'static str {
        self.inner.label()
    }

    fn load(&self, model_dir: &Path) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (holders, loaded) = &mut *state;
        if loaded.as_deref() != Some(model_dir) {
            // Un autre dossier : la part sur l'ancien est rendue d'abord.
            if loaded.take().is_some() {
                self.inner.release();
            }
            self.inner.load(model_dir)?;
            *loaded = Some(model_dir.to_path_buf());
        }
        *holders += 1;
        Ok(())
    }

    fn warm_up(&self) -> Result<()> {
        self.inner.warm_up()
    }

    fn infer(&self, ids: &[i64], style: &[f32]) -> Result<KokoroRaw> {
        self.inner.infer(ids, style)
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (holders, loaded) = &mut *state;
        if *holders == 0 {
            return;
        }
        *holders -= 1;
        if *holders == 0 && loaded.take().is_some() {
            self.inner.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::kokoro::phonemes::Phonemizer;
    use crate::tts::kokoro::KokoroEngine;
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Compte chargements, libérations et chargements simultanés.
    #[derive(Default)]
    struct Counting {
        loads: AtomicUsize,
        releases: AtomicUsize,
        in_flight: AtomicUsize,
        most_in_flight: AtomicUsize,
    }

    impl KokoroBackend for Counting {
        fn label(&self) -> &'static str {
            "compté"
        }
        fn load(&self, _: &Path) -> Result<()> {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_in_flight.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }
        fn warm_up(&self) -> Result<()> {
            Ok(())
        }
        fn infer(&self, _: &[i64], _: &[f32]) -> Result<KokoroRaw> {
            anyhow::bail!("non utilisé")
        }
        fn release(&self) {
            self.releases.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct Mute;
    impl Phonemizer for Mute {
        fn phonemize(&mut self, _: &str, items: &[String]) -> Result<Vec<String>> {
            Ok(items.to_vec())
        }
    }

    fn model_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-partage-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), r#"{"vocab": {"a": 43}}"#).unwrap();
        dir
    }

    fn engine(compute: &Arc<SharedCompute>, dir: &Path) -> KokoroEngine {
        let backend: Arc<dyn KokoroBackend> = Arc::clone(compute) as Arc<dyn KokoroBackend>;
        KokoroEngine::load(backend, Box::new(Mute), dir, Arc::new(SilentEngine)).unwrap()
    }

    #[test]
    fn an_old_engine_dropped_after_a_new_one_loaded_keeps_the_weights() {
        let raw = Arc::new(Counting::default());
        let compute = Arc::new(SharedCompute::new(Arc::clone(&raw) as Arc<dyn KokoroBackend>));
        let dir = model_dir("relais");
        let old = engine(&compute, &dir);
        let new = engine(&compute, &dir);
        assert_eq!(raw.loads.load(Ordering::SeqCst), 1, "mêmes poids : un seul chargement");
        drop(old);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 0, "le nouveau moteur s'en sert");
        drop(new);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 1);
        // Après la libération, un nouveau moteur recharge.
        let again = engine(&compute, &dir);
        assert_eq!(raw.loads.load(Ordering::SeqCst), 2);
        assert_eq!(again.name(), "Kokoro · compté");
    }

    #[test]
    fn another_model_dir_reloads() {
        let raw = Arc::new(Counting::default());
        let compute = Arc::new(SharedCompute::new(Arc::clone(&raw) as Arc<dyn KokoroBackend>));
        let a = engine(&compute, &model_dir("a"));
        let b = engine(&compute, &model_dir("b"));
        assert_eq!(raw.loads.load(Ordering::SeqCst), 2);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 1, "l'ancien dossier rendu avant le nouveau");
        drop(a);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 1, "le nouveau moteur s'en sert");
        drop(b);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 2, "toujours des paires load/release");
    }

    /// Un rechargement raté ne laisse rien à rendre : pas de `release` en
    /// trop pour le calcul (qui compterait mal ses autres détenteurs).
    #[test]
    fn a_failed_reload_leaves_nothing_to_release() {
        struct FailsOn(std::path::PathBuf, Counting);
        impl KokoroBackend for FailsOn {
            fn label(&self) -> &'static str {
                "raté"
            }
            fn load(&self, dir: &Path) -> Result<()> {
                anyhow::ensure!(dir != self.0, "illisible");
                self.1.load(dir)
            }
            fn warm_up(&self) -> Result<()> {
                Ok(())
            }
            fn infer(&self, _: &[i64], _: &[f32]) -> Result<KokoroRaw> {
                anyhow::bail!("non utilisé")
            }
            fn release(&self) {
                self.1.release()
            }
        }
        let bad = model_dir("raté");
        let raw = Arc::new(FailsOn(bad.clone(), Counting::default()));
        let compute = SharedCompute::new(Arc::clone(&raw) as Arc<dyn KokoroBackend>);
        compute.load(&model_dir("bon")).unwrap();
        assert!(compute.load(&bad).is_err());
        assert_eq!(raw.1.releases.load(Ordering::SeqCst), 1, "l'ancien rendu avant l'essai");
        compute.release();
        assert_eq!(raw.1.releases.load(Ordering::SeqCst), 1, "rien de plus à rendre");
    }

    #[test]
    fn concurrent_loads_are_serialised() {
        let raw = Arc::new(Counting::default());
        let compute = Arc::new(SharedCompute::new(Arc::clone(&raw) as Arc<dyn KokoroBackend>));
        let dirs: Vec<_> = (0..4).map(|i| model_dir(&format!("fil-{i}"))).collect();
        let handles: Vec<_> = dirs
            .into_iter()
            .map(|dir| {
                let compute = Arc::clone(&compute);
                std::thread::spawn(move || compute.load(&dir).unwrap())
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(raw.most_in_flight.load(Ordering::SeqCst), 1, "un chargement à la fois");
        // Quatre dossiers : quatre chargements, chacun rendu avant le suivant.
        assert_eq!(raw.loads.load(Ordering::SeqCst), 4);
        assert_eq!(raw.releases.load(Ordering::SeqCst), 3);
        for _ in 0..4 {
            compute.release();
        }
        assert_eq!(raw.releases.load(Ordering::SeqCst), 4, "toujours des paires load/release");
        compute.release();
        assert_eq!(raw.releases.load(Ordering::SeqCst), 4, "une libération de trop ne descend pas sous zéro");
    }

    #[test]
    fn an_absurd_output_length_is_refused() {
        let tokens = 3;
        let durations = vec![1_000u32; tokens];
        let raw = KokoroRaw { samples: vec![0.0; 3_000 * FRAME_FOR_TESTS], durations };
        assert!(check(&raw, tokens).is_err(), "1 000 trames par jeton : 25 s par phonème");
        let fine = KokoroRaw { samples: vec![0.0; 30 * FRAME_FOR_TESTS], durations: vec![10; 3] };
        assert!(check(&fine, tokens).is_ok());
    }

    const FRAME_FOR_TESTS: usize = super::super::FRAME;
}

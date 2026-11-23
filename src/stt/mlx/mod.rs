//! La dictée en MLX, en Rust (`mlx-rs`) : la traduction des modèles Swift
//! de `macos/Vendor/KokoroMLX/STT`, à parité avec eux. Les briques communes
//! (`dsp`, `nemo`, les couches de `nn`, la lecture des poids de `weights`)
//! servent à chaque modèle ; Parakeet v2 et v3 sont les premiers, Phonon-2
//! (le graphe de Parakeet, ses poids dans une archive) le suivant, puis
//! Whisper (tiny, small, large-v3-turbo), puis Canary 1b-v2 (l'encodeur de
//! Parakeet, un décodeur Transformer).
//!
//! Un modèle de dictée chargé inscrit son besoin dans les plafonds de MLX
//! sous la clé `"dictee"` (`need`), comme `SpeechModels.load` de l'app.

pub mod canary;
pub mod dsp;
pub mod nemo;
pub mod nn;
pub mod parakeet;
pub mod phonon;
pub mod weights;
pub mod whisper;

use crate::compute::mlx::budget::Need;
use std::path::Path;

/// La clé du modèle de dictée dans les plafonds de MLX.
pub const BUDGET_KEY: &str = "dictee";
/// La marge de mémoire au-delà des poids (`SpeechModels.memoryMargin`).
pub const MEMORY_MARGIN: usize = 2 << 30;
/// Le plafond du cache (`SpeechModels.cacheLimit`).
pub const CACHE_LIMIT: usize = 256 << 20;

/// Le besoin d'un modèle de dictée : ses poids plus 2 Gio, cache 256 Mio.
pub fn need(dir: &Path) -> Need {
    Need {
        memory: weight_bytes(dir) + MEMORY_MARGIN,
        cache: CACHE_LIMIT,
    }
}

/// Les octets des poids (`*.safetensors`, ou l'archive `.zst` de Phonon-2)
/// du dossier et de ses sous-dossiers, comme `MLXBudget.weightBytes`. Un
/// lien vers un fichier compte la taille du fichier. Un dossier illisible
/// compte pour zéro.
pub fn weight_bytes(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            total = total.saturating_add(weight_bytes(&path));
            continue;
        }
        let weight = path
            .extension()
            .is_some_and(|ext| ext == "safetensors" || ext == "zst");
        if weight {
            if let Ok(meta) = std::fs::metadata(&path) {
                if meta.is_file() {
                    total = total.saturating_add(usize::try_from(meta.len()).unwrap_or(usize::MAX));
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MLXBudgetTests.testWeightBytesCountSubfolders` : 10 + 5 octets, le
    /// vocabulaire ignoré.
    #[test]
    fn weight_bytes_count_subfolders() {
        let dir = crate::paths::test_scratch("stt-mlx-poids-sous-dossiers");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("speech_tokenizer")).unwrap();
        std::fs::write(dir.join("model.safetensors"), [0u8; 10]).unwrap();
        std::fs::write(dir.join("speech_tokenizer/model.safetensors"), [0u8; 5]).unwrap();
        std::fs::write(dir.join("vocab.json"), [0u8; 100]).unwrap();
        assert_eq!(weight_bytes(&dir), 15);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_need_is_the_weights_plus_two_gib() {
        let dir = crate::paths::test_scratch("stt-mlx-besoin");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.safetensors"), [0u8; 7]).unwrap();
        std::fs::write(dir.join("phonon.tar.zst"), [0u8; 3]).unwrap();
        assert_eq!(
            need(&dir),
            Need {
                memory: 10 + (2 << 30),
                cache: 256 << 20
            }
        );
        assert_eq!(weight_bytes(&dir.join("absent")), 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}

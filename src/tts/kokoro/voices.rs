//! Les voix de Kokoro : 510 × 256 flottants, une ligne de style par longueur
//! de phrase en phonèmes. Le format safetensors (Apache-2.0) est lu et écrit
//! à la main : un en-tête JSON, puis les octets.

use anyhow::{bail, ensure, Context, Result};
use std::path::Path;

pub const ROWS: usize = 510;
pub const WIDTH: usize = 256;
/// Colonnes 0–127 : le timbre ; 128–255 : la prosodie.
const TIMBRE: usize = 128;

/// Les voix livrées avec Sillon : nom et contenu.
const SHIPPED: &[(&str, &[u8])] = &[(
    "p-fenrir60",
    include_bytes!("../../../assets/voices/p-fenrir60.safetensors"),
)];

#[derive(Debug, Clone, PartialEq)]
pub struct VoicePack {
    values: Vec<f32>,
}

impl VoicePack {
    pub fn from_values(values: Vec<f32>) -> Result<Self> {
        ensure!(values.len() == ROWS * WIDTH, "voix illisible : {} valeurs au lieu de {}", values.len(), ROWS * WIDTH);
        Ok(Self { values })
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// La ligne de style d'une suite de `phonemes` phonèmes : `len − 1`,
    /// bornée au tableau.
    pub fn row(&self, phonemes: usize) -> &[f32] {
        let index = phonemes.saturating_sub(1).min(ROWS - 1);
        &self.values[index * WIDTH..(index + 1) * WIDTH]
    }

    /// Lit le seul tenseur d'un fichier safetensors, en flottants 32 bits.
    pub fn from_safetensors(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 8, "voix illisible : fichier trop court");
        let header_len = u64::from_le_bytes(bytes[..8].try_into().expect("8 octets")) as usize;
        ensure!(bytes.len() >= 8 + header_len, "voix illisible : en-tête tronqué");
        let header: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&bytes[8..8 + header_len]).context("voix illisible : en-tête")?;
        let (_, info) = header
            .iter()
            .find(|(name, _)| name.as_str() != "__metadata__")
            .context("voix illisible : aucun tenseur")?;
        let dtype = info["dtype"].as_str().unwrap_or("");
        if dtype != "F32" {
            bail!("voix illisible : type {dtype} au lieu de F32");
        }
        let offsets: Vec<usize> = info["data_offsets"]
            .as_array()
            .context("voix illisible : positions")?
            .iter()
            .filter_map(|v| v.as_u64().map(|v| v as usize))
            .collect();
        ensure!(offsets.len() == 2 && offsets[0] <= offsets[1], "voix illisible : positions");
        let start = 8 + header_len + offsets[0];
        let end = 8 + header_len + offsets[1];
        ensure!(end <= bytes.len() && end - start == ROWS * WIDTH * 4, "voix illisible : données tronquées");
        let values = bytes[start..end]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Self::from_values(values)
    }

    /// Écrit la voix comme celles du dépôt : un tenseur `voice` [510, 1, 256].
    pub fn to_safetensors(&self) -> Vec<u8> {
        let size = ROWS * WIDTH * 4;
        let mut header = format!(
            r#"{{"voice":{{"dtype":"F32","shape":[{ROWS},1,{WIDTH}],"data_offsets":[0,{size}]}}}}"#
        )
        .into_bytes();
        // L'en-tête se complète d'espaces jusqu'à un multiple de 8 octets.
        while header.len() % 8 != 0 {
            header.push(b' ');
        }
        let mut out = Vec::with_capacity(8 + header.len() + size);
        out.extend_from_slice(&(header.len() as u64).to_le_bytes());
        out.extend_from_slice(&header);
        for v in &self.values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// La recette de `p-fenrir60` : le timbre de `ff_siwis` (colonnes
    /// 0–127), la prosodie à 0,4 `ff_siwis` + 0,6 `am_fenrir` (128–255).
    pub fn blend_recipe(siwis: &VoicePack, fenrir: &VoicePack) -> VoicePack {
        let mut values = siwis.values.clone();
        for row in 0..ROWS {
            for col in TIMBRE..WIDTH {
                let i = row * WIDTH + col;
                values[i] = 0.4 * siwis.values[i] + 0.6 * fenrir.values[i];
            }
        }
        VoicePack { values }
    }
}

/// Une voix : livrée avec Sillon, sinon dans `model_dir/voices/`.
pub fn load_voice(model_dir: &Path, name: &str) -> Result<VoicePack> {
    if let Some((_, bytes)) = SHIPPED.iter().find(|(shipped, _)| *shipped == name) {
        return VoicePack::from_safetensors(bytes);
    }
    let path = model_dir.join("voices").join(format!("{name}.safetensors"));
    let bytes = std::fs::read(&path).with_context(|| format!("voix absente : {}", path.display()))?;
    VoicePack::from_safetensors(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> VoicePack {
        // La ligne r porte la valeur r : la ligne choisie se lit d'un coup d'œil.
        VoicePack::from_values((0..ROWS * WIDTH).map(|i| (i / WIDTH) as f32).collect()).unwrap()
    }

    #[test]
    fn the_style_row_is_the_phoneme_count_minus_one() {
        let v = ramp();
        assert_eq!(v.row(1)[0], 0.0);
        assert_eq!(v.row(37)[0], 36.0);
        assert_eq!(v.row(0)[0], 0.0, "rien à dire : la première ligne");
        assert_eq!(v.row(900)[0], 509.0, "au-delà : la dernière");
        assert_eq!(v.row(37).len(), WIDTH);
    }

    #[test]
    fn a_voice_round_trips_through_safetensors() {
        let v = ramp();
        let bytes = v.to_safetensors();
        assert_eq!(bytes.len() as u64, crate::models::KOKORO_VOICE_BYTES, "même taille que les voix du dépôt");
        assert_eq!(VoicePack::from_safetensors(&bytes).unwrap(), v);
    }

    #[test]
    fn a_broken_file_is_refused_with_a_reason() {
        assert!(VoicePack::from_safetensors(b"court").is_err());
        let mut bytes = ramp().to_safetensors();
        bytes.truncate(bytes.len() - 4);
        assert!(VoicePack::from_safetensors(&bytes).is_err());
        assert!(VoicePack::from_values(vec![0.0; 10]).is_err());
    }

    #[test]
    fn the_recipe_keeps_the_timbre_and_mixes_the_prosody() {
        let siwis = VoicePack::from_values(vec![1.0; ROWS * WIDTH]).unwrap();
        let fenrir = VoicePack::from_values(vec![2.0; ROWS * WIDTH]).unwrap();
        let blend = VoicePack::blend_recipe(&siwis, &fenrir);
        let row = blend.row(10);
        assert_eq!(row[0], 1.0);
        assert_eq!(row[127], 1.0, "timbre : ff_siwis");
        assert!((row[128] - 1.6).abs() < 1e-6, "prosodie : 0,4 × 1 + 0,6 × 2");
        assert!((row[255] - 1.6).abs() < 1e-6);
    }

    #[test]
    fn the_french_voice_ships_with_sillon() {
        let dir = std::env::temp_dir().join("sillon-sans-voix");
        let v = load_voice(&dir, "p-fenrir60").expect("livrée dans le binaire");
        assert_eq!(v.values().len(), ROWS * WIDTH);
        assert!(load_voice(&dir, "af_bella").is_err(), "les autres viennent du paquet");
    }

    /// `SILLON_KOKORO_DIR=<dossier avec voices/ff_siwis.safetensors et
    /// voices/am_fenrir.safetensors> cargo test --lib kokoro::voices -- --ignored`
    #[test]
    #[ignore]
    fn the_shipped_voice_matches_its_recipe() {
        let dir = std::path::PathBuf::from(std::env::var("SILLON_KOKORO_DIR").expect("SILLON_KOKORO_DIR"));
        let read = |name: &str| {
            VoicePack::from_safetensors(&std::fs::read(dir.join("voices").join(format!("{name}.safetensors"))).unwrap())
                .unwrap()
        };
        let recomputed = VoicePack::blend_recipe(&read("ff_siwis"), &read("am_fenrir"));
        let shipped = load_voice(&dir, "p-fenrir60").unwrap();
        let worst = recomputed
            .values()
            .iter()
            .zip(shipped.values())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst <= 1e-6, "{worst}");
    }
}

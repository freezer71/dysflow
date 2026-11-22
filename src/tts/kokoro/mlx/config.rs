//! `config.json` et les poids de Kokoro : lus, vérifiés (clé présente,
//! forme attendue) et mis dans la disposition de MLX, comme
//! `KokoroConfig.swift` et `KokoroModel.load`/`sanitize`.

use anyhow::{anyhow, Context, Result};
use mlx_rs::Array;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "config.json";
/// Le fichier unique, s'il est là ; sinon tous les `*.safetensors` du
/// dossier hors voix (`KokoroModel.loadWeights`).
const SINGLE_FILE: &str = "model.safetensors";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub hidden_dim: i32,
    #[serde(default = "defaults::max_dur")]
    pub max_dur: i32,
    #[serde(default = "defaults::n_layer")]
    pub n_layer: i32,
    pub n_token: i32,
    #[serde(default = "defaults::style_dim")]
    pub style_dim: i32,
    #[serde(default = "defaults::text_encoder_kernel_size")]
    pub text_encoder_kernel_size: i32,
    #[serde(default = "defaults::asr_res_dim")]
    pub asr_res_dim: i32,
    pub plbert: Plbert,
    pub istftnet: Istftnet,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Plbert {
    pub num_hidden_layers: i32,
    pub num_attention_heads: i32,
    pub hidden_size: i32,
    pub intermediate_size: i32,
    pub max_position_embeddings: i32,
    #[serde(default = "defaults::embedding_size")]
    pub embedding_size: i32,
    #[serde(default = "defaults::one")]
    pub inner_group_num: i32,
    #[serde(default = "defaults::one")]
    pub num_hidden_groups: i32,
    #[serde(default = "defaults::type_vocab_size")]
    pub type_vocab_size: i32,
    #[serde(default = "defaults::layer_norm_eps")]
    pub layer_norm_eps: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Istftnet {
    pub resblock_kernel_sizes: Vec<i32>,
    pub upsample_rates: Vec<i32>,
    pub upsample_initial_channel: i32,
    pub resblock_dilation_sizes: Vec<Vec<i32>>,
    pub upsample_kernel_sizes: Vec<i32>,
    pub gen_istft_n_fft: i32,
    pub gen_istft_hop_size: i32,
}

/// Les valeurs par défaut de `KokoroConfig.swift`.
mod defaults {
    pub fn max_dur() -> i32 {
        50
    }
    pub fn n_layer() -> i32 {
        3
    }
    pub fn style_dim() -> i32 {
        128
    }
    pub fn text_encoder_kernel_size() -> i32 {
        5
    }
    pub fn asr_res_dim() -> i32 {
        64
    }
    pub fn embedding_size() -> i32 {
        128
    }
    pub fn one() -> i32 {
        1
    }
    pub fn type_vocab_size() -> i32 {
        2
    }
    pub fn layer_norm_eps() -> f32 {
        1e-12
    }
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", path.display()))?;
        let config: Self = serde_json::from_str(&text).with_context(|| {
            crate::tr!(en = "unreadable Kokoro configuration ({})", fr = "configuration de Kokoro illisible ({})", path.display())
        })?;
        config.check().with_context(|| path.display().to_string())?;
        Ok(config)
    }

    /// Ce que le calcul suppose : sinon une division par zéro ou un index
    /// hors des tableaux plus loin.
    fn check(&self) -> Result<()> {
        let net = &self.istftnet;
        let bert = &self.plbert;
        let fine = bert.num_attention_heads > 0
            && bert.hidden_size % bert.num_attention_heads == 0
            && bert.num_hidden_groups > 0
            && bert.num_hidden_layers % bert.num_hidden_groups == 0
            && bert.num_hidden_layers >= bert.num_hidden_groups
            && !net.upsample_rates.is_empty()
            && net.upsample_rates.len() == net.upsample_kernel_sizes.len()
            && net.resblock_kernel_sizes.len() == net.resblock_dilation_sizes.len()
            && !net.resblock_kernel_sizes.is_empty()
            && net.upsample_rates.iter().all(|&rate| rate > 0)
            && net.gen_istft_hop_size > 0
            && net.gen_istft_n_fft >= net.gen_istft_hop_size
            && net.gen_istft_n_fft % net.gen_istft_hop_size == 0
            && self.style_dim > 0
            && self.hidden_dim > 0;
        anyhow::ensure!(fine, crate::tr!("inconsistent Kokoro configuration", "configuration de Kokoro incohérente"));
        Ok(())
    }
}

/// Les tenseurs lus, par clé, chacun avec le fichier d'où il vient. Chaque
/// clé est prise une fois, avec sa forme attendue dans la disposition de
/// PyTorch (celle du fichier) ; la prise la met dans celle de MLX.
pub struct Weights {
    tensors: HashMap<String, (Array, PathBuf)>,
}

impl Weights {
    /// Lit et évalue les fichiers de poids du dossier.
    pub fn load(dir: &Path) -> Result<Self> {
        let mut tensors = HashMap::new();
        for file in weight_files(dir)? {
            let arrays = Array::load_safetensors(&file).map_err(|err| {
                anyhow!(crate::tr!(
                    en = "unreadable Kokoro weights ({}): {err}",
                    fr = "poids de Kokoro illisibles ({}) : {err}",
                    file.display(),
                ))
            })?;
            // Lus ici, comme `MLX.eval(model.parameters())` du Swift : des
            // données tronquées sont une erreur qui nomme le fichier.
            mlx_rs::transforms::eval(arrays.values()).map_err(|err| {
                anyhow!(crate::tr!(
                    en = "unreadable Kokoro weights ({}): {err}",
                    fr = "poids de Kokoro illisibles ({}) : {err}",
                    file.display(),
                ))
            })?;
            for (key, array) in arrays {
                if key.contains("position_ids") {
                    continue;
                }
                tensors.insert(key, (array, file.clone()));
            }
        }
        Ok(Self { tensors })
    }

    /// Un tenseur tel quel (biais, normalisations, plongements, `Linear`).
    pub fn take(&mut self, key: &str, shape: &[i32]) -> Result<Array> {
        let (array, file) = self.tensors.remove(key).ok_or_else(|| {
            anyhow!(crate::tr!(
                en = "Kokoro weights: key {key} is missing",
                fr = "poids de Kokoro : la clé {key} manque",
            ))
        })?;
        if array.shape() != shape {
            return Err(anyhow!(crate::tr!(
                en = "Kokoro weights ({}): {key} has the shape {:?} instead of {:?}",
                fr = "poids de Kokoro ({}) : {key} a la forme {:?} au lieu de {:?}",
                file.display(),
                array.shape(),
                shape,
            )));
        }
        Ok(array)
    }

    /// Le poids d'un `Conv1d` de PyTorch, `[sortie, entrée, noyau]`, rendu
    /// en `[sortie, noyau, entrée]` (`F0_proj`, `N_proj`, `noise_convs`).
    pub fn take_conv(&mut self, key: &str, shape: [i32; 3]) -> Result<Array> {
        Ok(self.take(key, &shape)?.transpose_axes(&[0, 2, 1])?)
    }

    /// Le `weight_v` d'une convolution normalisée : transposé comme le fait
    /// `sanitize`, sauf s'il a déjà la forme de MLX selon sa règle.
    pub fn take_weight_v(&mut self, key: &str, shape: [i32; 3]) -> Result<Array> {
        let value = self.take(key, &shape)?;
        let [o, h, w] = shape;
        if o >= h && o >= w && h == w {
            return Ok(value);
        }
        Ok(value.transpose_axes(&[0, 2, 1])?)
    }
}

/// `model.safetensors` s'il existe, sinon les `*.safetensors` du dossier
/// dont le nom ne contient pas « voices », dans l'ordre des noms.
fn weight_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let single = dir.join(SINGLE_FILE);
    if single.is_file() {
        return Ok(vec![single]);
    }
    let entries = std::fs::read_dir(dir)
        .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "safetensors")
                && path.file_name().is_some_and(|name| !name.to_string_lossy().contains("voices"))
        })
        .collect();
    files.sort();
    anyhow::ensure!(
        !files.is_empty(),
        crate::tr!(
            en = "no Kokoro weights (*.safetensors) in {}",
            fr = "aucun poids de Kokoro (*.safetensors) dans {}",
            dir.display(),
        )
    );
    Ok(files)
}

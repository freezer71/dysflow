//! Les poids d'un modèle de dictée MLX : les `*.safetensors` du dossier,
//! clés renommées vers celles du graphe, flottants au type de calcul, puis
//! pris un à un avec leur forme attendue. Comme `fromDirectory` du Swift :
//! `loadArrays`, `sanitize`, la passe qui met chaque flottant au type de
//! calcul, et `update(parameters:verify: .all)` (clé absente, clé en trop
//! ou forme inattendue : une erreur).

use anyhow::{anyhow, Context, Result};
use mlx_rs::{Array, Dtype};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// La quantification affine lue dans `config.json` (`"quantization":
/// {"group_size": …, "bits": …}`) ; jamais écrite en dur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Quantization {
    pub group_size: i32,
    pub bits: i32,
}

/// Les tenseurs lus, par clé du graphe, chacun avec le fichier d'où il
/// vient. `model` nomme le modèle dans les messages (« Parakeet »).
pub struct Weights {
    model: &'static str,
    tensors: BTreeMap<String, (Array, PathBuf)>,
    pub quantization: Option<Quantization>,
}

impl Weights {
    /// Lit les `*.safetensors` de `dir`, dans l'ordre des noms (une clé de
    /// deux fichiers : le dernier l'emporte, comme `merge` du Swift).
    /// `rename` donne la clé du graphe, ou `None` pour une clé ignorée. Les
    /// flottants passent à `compute` (les mots quantifiés `uint32` restent
    /// tels quels), et chaque fichier est évalué ici : des données tronquées
    /// sont une erreur qui le nomme.
    pub fn load(
        dir: &Path,
        model: &'static str,
        compute: Dtype,
        quantization: Option<Quantization>,
        rename: impl Fn(&str) -> Option<String>,
    ) -> Result<Self> {
        let mut tensors = BTreeMap::new();
        for file in weight_files(dir, model)? {
            let unreadable = |err: &dyn std::fmt::Display| {
                anyhow!(crate::tr!(
                    en = "unreadable {model} weights ({}): {err}",
                    fr = "poids de {model} illisibles ({}) : {err}",
                    file.display(),
                ))
            };
            let arrays = Array::load_safetensors(&file).map_err(|err| unreadable(&err))?;
            let mut read = Vec::with_capacity(arrays.len());
            for (key, array) in arrays {
                let Some(key) = rename(&key) else { continue };
                let array = if array.dtype().is_float() && array.dtype() != compute {
                    array.as_dtype(compute).map_err(|err| unreadable(&err))?
                } else {
                    array
                };
                read.push((key, array));
            }
            mlx_rs::transforms::eval(read.iter().map(|(_, array)| array))
                .map_err(|err| unreadable(&err))?;
            for (key, array) in read {
                tensors.insert(key, (array, file.clone()));
            }
        }
        Ok(Self {
            model,
            tensors,
            quantization,
        })
    }

    /// La clé est là (une couche quantifiée a ses `scales`).
    pub fn has(&self, key: &str) -> bool {
        self.tensors.contains_key(key)
    }

    /// Le tenseur de `key`, de forme `shape`.
    pub fn take(&mut self, key: &str, shape: &[i32]) -> Result<Array> {
        let model = self.model;
        let (array, file) = self.tensors.remove(key).ok_or_else(|| {
            anyhow!(crate::tr!(
                en = "{model} weights: key {key} is missing",
                fr = "poids de {model} : la clé {key} manque",
            ))
        })?;
        if array.shape() != shape {
            return Err(anyhow!(crate::tr!(
                en = "{model} weights ({}): {key} has the shape {:?} instead of {:?}",
                fr = "poids de {model} ({}) : {key} a la forme {:?} au lieu de {:?}",
                file.display(),
                array.shape(),
                shape,
            )));
        }
        Ok(array)
    }

    /// Tout a été pris : une clé en trop est une erreur qui la nomme
    /// (`verify: .noUnusedKeys`).
    pub fn finish(self) -> Result<()> {
        let model = self.model;
        if let Some((key, (_, file))) = self.tensors.iter().next() {
            let count = self.tensors.len();
            return Err(anyhow!(crate::tr!(
                en = "{model} weights ({}): {count} key(s) unknown to the model, {key} first",
                fr = "poids de {model} ({}) : {count} clé(s) inconnue(s) du modèle, dont {key}",
                file.display(),
            )));
        }
        Ok(())
    }
}

/// Les `*.safetensors` de `dir`, dans l'ordre des noms ; aucun : une erreur.
fn weight_files(dir: &Path, model: &str) -> Result<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "safetensors"))
        .collect();
    files.sort();
    anyhow::ensure!(
        !files.is_empty(),
        crate::tr!(
            en = "no {model} weights (*.safetensors) in {}",
            fr = "aucun poids de {model} (*.safetensors) dans {}",
            dir.display(),
        )
    );
    Ok(files)
}

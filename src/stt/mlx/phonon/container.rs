//! Le conteneur `fermion-five-value-parakeet-v1` de Phonon-2
//! (`model.fermion`) : un en-tête JSON (sa taille sur 8 octets
//! petit-boutistes, puis l'index), puis les enregistrements bout à bout,
//! nommés comme `ParakeetForTDT` de transformers. Trois sortes :
//! - `five_value` [o, i] : un trit par poids (5 par octet, chiffre 1 = zéro,
//!   chaque ligne complétée à l'octet), puis un bit par poids non nul (1 :
//!   magnitude hi), puis lo et hi par ligne en float16 ;
//! - `int6` : entiers de 6 bits (4 dans 3 octets, décalés de 32), une échelle
//!   float16 par ligne en fin d'enregistrement ;
//! - `fp16` : tel quel.
//!
//! Port de `PhononContainer` (`PhononLoader.swift`), référence
//! `fermion_container.py` du dépôt FermionResearch/Phonon-2. Les tableaux
//! sortent sous les clés du graphe de Parakeet (`graph_key`), à la
//! disposition de MLX, dans leur type du fichier : `Weights::from_arrays` les
//! met ensuite au type de calcul, comme la passe du Swift.
//!
//! Le Swift déroule trits et bits en opérations MLX ; ici, une boucle Rust
//! les déroule. Ce sont des entiers : codes, mots et produits `int6`
//! (q × échelle, |q| ≤ 32, exact en `f32`) sont les mêmes à l'octet. Les
//! opérations flottantes (échelles par groupe, leur opposé, la matrice
//! dense des convolutions ponctuelles, la somme des biais du LSTM) restent
//! celles de MLX, dans le même ordre. Un octet de trits au-delà de 242,
//! qu'un `take` MLX lirait hors de sa table, est ici une erreur.

use anyhow::{anyhow, Result};
use half::f16;
use mlx_rs::ops::DequantizeOptions;
use mlx_rs::{ops, Array};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::stt::mlx::nn::{FIVE_VALUE_GROUP_SIZE, FIVE_VALUE_HI};

/// Le format attendu dans l'en-tête (`PhononContainer.format`).
pub const FORMAT: &str = "fermion-five-value-parakeet-v1";

#[derive(Deserialize)]
struct Record {
    n: String,
    k: String,
    shape: Vec<i64>,
    b: u64,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    index: Vec<Record>,
}

/// Le conteneur `data`, lu depuis `source` (nommé dans les erreurs) : les
/// tableaux sous les clés du graphe. Une couche à cinq valeurs donne ses
/// deux plans (`{clé}.weight`, `.scales`, `.biases`, puis les mêmes sous
/// `{clé}.hi`), sauf une convolution ponctuelle, qui reste une `Conv1d` à
/// poids dense `[o, 1, i]`.
pub fn decode(data: &[u8], source: &Path) -> Result<Vec<(String, Array)>> {
    let malformed = |detail: String| {
        anyhow!(crate::tr!(
            en = "invalid Phonon container ({}): {detail}",
            fr = "conteneur Phonon invalide ({}) : {detail}",
            source.display(),
        ))
    };
    let Some((size, rest)) = data.split_first_chunk::<8>() else {
        return Err(malformed(crate::tr!("too short", "trop court").into()));
    };
    let header_size = usize::try_from(u64::from_le_bytes(*size)).unwrap_or(usize::MAX);
    if header_size == 0 || header_size > rest.len() {
        return Err(malformed(crate::tr!("header", "en-tête").into()));
    }
    let (header, mut body) = rest.split_at(header_size);
    let header: Header = serde_json::from_slice(header)
        .map_err(|err| malformed(crate::tr!(en = "header: {err}", fr = "en-tête : {err}")))?;
    if header.format != FORMAT {
        return Err(malformed(crate::tr!(
            en = "format {}",
            fr = "format {}",
            header.format
        )));
    }
    let announced = header
        .index
        .iter()
        .try_fold(0u64, |sum, r| sum.checked_add(r.b));
    if announced != Some(body.len() as u64) {
        return Err(malformed(crate::tr!(
            en = "size {}",
            fr = "taille {}",
            data.len()
        )));
    }

    let mut arrays = Vec::new();
    let mut lstm_biases: BTreeMap<String, [Option<Array>; 2]> = BTreeMap::new();
    for record in &header.index {
        // Les tailles somment à celle du corps : chaque `b` y tient.
        let (blob, next) = body.split_at(record.b as usize);
        body = next;
        let shape = dims(&record.shape)
            .ok_or_else(|| malformed(format!("{} {:?}", record.n, record.shape)))?;
        let value = match record.k.as_str() {
            "five_value" => {
                let [rows, columns] = shape[..] else {
                    return Err(malformed(record.n.clone()));
                };
                let planes = planes(blob, rows, columns, &record.n).map_err(malformed)?;
                let key = graph_key(&record.n);
                let layer = FiveValueLayer::new(planes)?;
                if key.contains(".pointwise_conv") {
                    // Les convolutions de noyau 1 restent des `Conv1d` [o, 1, i].
                    let dense = layer.dense()?.expand_dims(1)?;
                    dense.eval()?;
                    arrays.push((format!("{key}.weight"), dense));
                } else {
                    layer.eval()?;
                    arrays.extend(layer.into_arrays(&key));
                }
                continue;
            }
            "int6" => {
                let values = int_six(blob, &shape).map_err(malformed)?;
                Array::from_slice(&values, &mlx_shape(&shape))
            }
            "fp16" => {
                if Some(blob.len()) != shape.iter().product::<usize>().checked_mul(2) {
                    return Err(malformed(record.n.clone()));
                }
                Array::from_slice(&halves(blob), &mlx_shape(&shape))
            }
            kind => {
                return Err(malformed(crate::tr!(
                    en = "kind {kind} ({})",
                    fr = "sorte {kind} ({})",
                    record.n
                )))
            }
        };
        store(&record.n, value, &mut arrays, &mut lstm_biases)?;
    }

    // LSTM : un seul biais dans le graphe, la somme des deux de PyTorch.
    for (layer, [ih, hh]) in lstm_biases {
        let (Some(ih), Some(hh)) = (ih, hh) else {
            return Err(malformed(crate::tr!(
                en = "LSTM biases {layer}",
                fr = "biais du LSTM {layer}"
            )));
        };
        let bias = ih.add(&hh)?;
        bias.eval()?;
        arrays.push((format!("{layer}.bias"), bias));
    }
    Ok(arrays)
}

/// Une forme du fichier : des dimensions positives, chacune dans un `i32`
/// (MLX), et un produit qui tient dans un `usize`.
fn dims(shape: &[i64]) -> Option<Vec<usize>> {
    let dims: Vec<usize> = shape
        .iter()
        .map(|&d| i32::try_from(d).ok().and_then(|d| usize::try_from(d).ok()))
        .collect::<Option<_>>()?;
    dims.iter()
        .try_fold(1usize, |p, &d| p.checked_mul(d))
        .map(|_| dims)
}

fn mlx_shape(shape: &[usize]) -> Vec<i32> {
    // `dims` a vérifié que chaque dimension tient dans un i32.
    shape.iter().map(|&d| d as i32).collect()
}

/// Des float16 petit-boutistes.
fn halves(bytes: &[u8]) -> Vec<f16> {
    bytes
        .chunks_exact(2)
        .map(|b| f16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// Range un tableau dense sous son nom du graphe, à la disposition de MLX
/// (canaux en dernier pour les convolutions), comme `store` du Swift.
fn store(
    name: &str,
    value: Array,
    arrays: &mut Vec<(String, Array)>,
    lstm_biases: &mut BTreeMap<String, [Option<Array>; 2]>,
) -> Result<()> {
    // Compteur de BatchNorm, jamais lu en inférence.
    if name.ends_with(".num_batches_tracked") {
        return Ok(());
    }
    if let Some((kind, layer)) = lstm_part(name, "decoder.lstm.bias_") {
        let slot = usize::from(kind == "hh");
        lstm_biases
            .entry(format!("decoder.prediction.dec_rnn.lstm.{layer}"))
            .or_default()[slot] = Some(value);
        return Ok(());
    }
    let array = match value.ndim() {
        3 => value.transpose_axes(&[0, 2, 1])?,
        4 => value.transpose_axes(&[0, 2, 3, 1])?,
        _ => value,
    };
    array.eval()?;
    arrays.push((graph_key(name), array));
    Ok(())
}

/// `{prefix}(ih|hh)_l{n}` : la sorte et la couche, comme l'expression
/// `^decoder\.lstm\.(bias|weight)_(ih|hh)_l\d+$` du Swift.
fn lstm_part<'a>(name: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let (kind, layer) = name.strip_prefix(prefix)?.split_once("_l")?;
    let digits = !layer.is_empty() && layer.bytes().all(|b| b.is_ascii_digit());
    (matches!(kind, "ih" | "hh") && digits).then_some((kind, layer))
}

/// Le nom d'un tenseur de transformers dans le graphe : les noms de NeMo
/// (ceux des safetensors de mlx-community), puis les renommages de
/// `remap_key` (`joint_net.2` → `joint_net`, `posBiasU`, liste du
/// sous-échantillonnage → `conv0`, `depthwise_layers.n`,
/// `pointwise_layers.n`). `PhononContainer.graphKey` du Swift.
pub fn graph_key(name: &str) -> String {
    let mut key = name.to_string();
    for (old, new) in [
        ("encoder.subsampling.linear", "encoder.pre_encode.out"),
        (".conv.norm.", ".conv.batch_norm."),
        (".self_attn.q_proj", ".self_attn.linear_q"),
        (".self_attn.k_proj", ".self_attn.linear_k"),
        (".self_attn.v_proj", ".self_attn.linear_v"),
        (".self_attn.o_proj", ".self_attn.linear_out"),
        (".self_attn.relative_k_proj", ".self_attn.linear_pos"),
        (".self_attn.bias_u", ".self_attn.posBiasU"),
        (".self_attn.bias_v", ".self_attn.posBiasV"),
        ("decoder.embedding.", "decoder.prediction.embed."),
        ("decoder.decoder_projector.", "joint.pred."),
        ("encoder_projector.", "joint.enc."),
        ("joint.head.", "joint.joint_net."),
    ] {
        key = key.replace(old, new);
    }
    if let Some(rest) = key.strip_prefix("encoder.subsampling.layers.") {
        // conv.0, puis par bloc : profondeur (2 + 3n), point (3 + 3n), ReLU
        // (4 + 3n). Division et reste tronqués vers zéro, comme en Swift.
        if let Some((index, suffix)) = rest.split_once('.') {
            if let Ok(index) = index.parse::<i64>() {
                let layer = if index == 0 {
                    "conv0".to_string()
                } else if (index - 2) % 3 == 0 {
                    format!("depthwise_layers.{}", (index - 2) / 3)
                } else {
                    format!("pointwise_layers.{}", (index - 2) / 3)
                };
                key = format!("encoder.pre_encode.{layer}.{suffix}");
            }
        }
    }
    if let Some((kind, layer)) = lstm_part(&key, "decoder.lstm.weight_") {
        let weight = if kind == "ih" { "Wx" } else { "Wh" };
        key = format!("decoder.prediction.dec_rnn.lstm.{layer}.{weight}");
    }
    key
}

/// `int6` : q × échelle de sa ligne, en `f32` (`PhononContainer.intSix`).
pub fn int_six(blob: &[u8], shape: &[usize]) -> std::result::Result<Vec<f32>, String> {
    let Some(&rows) = shape.first().filter(|&&rows| rows > 0) else {
        return Err(crate::tr!("int6 without rows", "int6 sans lignes").into());
    };
    let total: usize = shape.iter().product();
    let wrong = || {
        crate::tr!(
            en = "int6 of {} bytes for {shape:?}",
            fr = "int6 de {} octets pour {shape:?}",
            blob.len()
        )
    };
    let body_size = blob.len().checked_sub(2 * rows).ok_or_else(wrong)?;
    if body_size % 3 != 0 || body_size / 3 * 4 < total {
        return Err(wrong());
    }
    let (body, scales) = blob.split_at(body_size);
    let scales: Vec<f32> = halves(scales).into_iter().map(f32::from).collect();
    let columns = total / rows;
    Ok((0..total)
        .map(|i| {
            let b = &body[i / 4 * 3..i / 4 * 3 + 3];
            let packed = u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16;
            let q = ((packed >> (6 * (i % 4))) & 0x3F) as i32 - 32;
            q as f32 * scales[i / columns]
        })
        .collect())
}

/// Les deux plans d'une matrice à cinq valeurs : codes 2 bits rangés par 16
/// dans des `u32` [o, i / 16], le premier dans les bits de poids faible (le
/// rangement de MLX), et les magnitudes par ligne (float16).
pub struct Planes {
    pub rows: usize,
    pub columns: usize,
    pub lo: Vec<u32>,
    pub hi: Vec<u32>,
    pub lo_scale: Vec<f16>,
    pub hi_scale: Vec<f16>,
}

/// `five_value` [rows, columns] → deux plans 2 bits. Code 1 + signe sur le
/// plan de sa magnitude, 1 (zéro) sur l'autre (`PhononContainer.planes`).
pub fn planes(
    blob: &[u8],
    rows: usize,
    columns: usize,
    name: &str,
) -> std::result::Result<Planes, String> {
    let group = FIVE_VALUE_GROUP_SIZE as usize;
    if rows == 0 || columns == 0 || !columns.is_multiple_of(group) {
        return Err(crate::tr!(
            en = "shape [{rows}, {columns}] of {name}",
            fr = "forme [{rows}, {columns}] de {name}"
        ));
    }
    let row_bytes = columns.div_ceil(5);
    let trit_bytes = rows.saturating_mul(row_bytes);
    if blob.len() < trit_bytes {
        return Err(name.to_string());
    }
    // Les codes 0, 1, 2 (signe + 1), chiffre de poids faible d'abord ; le
    // remplissage de fin de ligne tombe.
    let mut codes = Vec::with_capacity(rows * columns);
    for row in blob[..trit_bytes].chunks_exact(row_bytes) {
        let start = codes.len();
        for &byte in row {
            if byte > 242 {
                return Err(crate::tr!(
                    en = "{name}: byte {byte} is not five trits",
                    fr = "{name} : l'octet {byte} n'est pas cinq trits"
                ));
            }
            let mut rest = byte;
            for _ in 0..5 {
                codes.push(rest % 3);
                rest /= 3;
            }
        }
        codes.truncate(start + columns);
    }
    let count = codes.iter().filter(|&&c| c != 1).count();
    let scales_start = trit_bytes + count.div_ceil(8);
    if scales_start.checked_add(4 * rows) != Some(blob.len()) {
        return Err(crate::tr!(
            en = "{name}: {} bytes, {} expected",
            fr = "{name} : {} octets, {} attendus",
            blob.len(),
            scales_start.saturating_add(4 * rows)
        ));
    }
    // Un bit par non-nul (dans l'ordre des lignes), poids faible d'abord :
    // 1 = magnitude hi. `count` non-nuls tiennent dans ces octets.
    let mut magnitudes = blob[trit_bytes..scales_start]
        .iter()
        .flat_map(|&byte| (0..8).map(move |k| byte >> k & 1 == 1));
    let (mut lo, mut hi) = (
        vec![0u32; rows * columns / 16],
        vec![0u32; rows * columns / 16],
    );
    for (i, &code) in codes.iter().enumerate() {
        let is_hi = code != 1 && magnitudes.next() == Some(true);
        let (lo_code, hi_code) = if is_hi { (1, code) } else { (code, 1) };
        let shift = 2 * (i % 16);
        lo[i / 16] |= u32::from(lo_code) << shift;
        hi[i / 16] |= u32::from(hi_code) << shift;
    }
    let scales = halves(&blob[scales_start..]);
    let (lo_scale, hi_scale) = scales.split_at(rows);
    Ok(Planes {
        rows,
        columns,
        lo,
        hi,
        lo_scale: lo_scale.to_vec(),
        hi_scale: hi_scale.to_vec(),
    })
}

/// `FiveValueLinear` du Swift : les mots des deux plans, et par groupe de
/// 128 l'échelle de la ligne et son opposé (codes 0, 1, 2 → −s, 0, s).
pub struct FiveValueLayer {
    lo: Array,
    hi: Array,
    lo_scales: Array,
    lo_biases: Array,
    hi_scales: Array,
    hi_biases: Array,
}

impl FiveValueLayer {
    pub fn new(planes: Planes) -> Result<Self> {
        let (rows, columns) = (planes.rows as i32, planes.columns as i32);
        let groups = columns / FIVE_VALUE_GROUP_SIZE;
        let per_group = |scale: &[f16]| -> Result<Array> {
            let scale = Array::from_slice(scale, &[rows]).expand_dims(1)?;
            Ok(Array::repeat_axis::<f16>(scale, groups, 1)?)
        };
        let (lo_scales, hi_scales) = (per_group(&planes.lo_scale)?, per_group(&planes.hi_scale)?);
        Ok(Self {
            lo: Array::from_slice(&planes.lo, &[rows, columns / 16]),
            hi: Array::from_slice(&planes.hi, &[rows, columns / 16]),
            lo_biases: ops::negative(&lo_scales)?,
            hi_biases: ops::negative(&hi_scales)?,
            lo_scales,
            hi_scales,
        })
    }

    /// La matrice dense [rows, columns] (exacte en float16) : les deux plans
    /// déquantifiés, sommés (`dense()` du Swift).
    pub fn dense(&self) -> Result<Array> {
        let options = DequantizeOptions {
            group_size: Some(FIVE_VALUE_GROUP_SIZE),
            bits: Some(2),
            ..Default::default()
        };
        let lo = ops::dequantize(&self.lo, &self.lo_scales, &self.lo_biases, options)?;
        let hi = ops::dequantize(&self.hi, &self.hi_scales, &self.hi_biases, options)?;
        Ok(lo.add(&hi)?)
    }

    /// Un enregistrement à la fois : les intermédiaires ne s'accumulent pas.
    pub fn eval(&self) -> Result<()> {
        mlx_rs::transforms::eval([
            &self.lo,
            &self.hi,
            &self.lo_scales,
            &self.lo_biases,
            &self.hi_scales,
            &self.hi_biases,
        ])?;
        Ok(())
    }

    /// Les clés que `nn::Linear::load` lit pour la couche `key`.
    pub fn into_arrays(self, key: &str) -> [(String, Array); 6] {
        [
            (format!("{key}.weight"), self.lo),
            (format!("{key}.scales"), self.lo_scales),
            (format!("{key}.biases"), self.lo_biases),
            (format!("{key}.{FIVE_VALUE_HI}.weight"), self.hi),
            (format!("{key}.{FIVE_VALUE_HI}.scales"), self.hi_scales),
            (format!("{key}.{FIVE_VALUE_HI}.biases"), self.hi_biases),
        ]
    }
}

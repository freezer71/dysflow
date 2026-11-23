//! Les essais de Phonon-2 en MLX. Ceux qui lancent le vrai modèle sont
//! ignorés par défaut : `DYSFLOW_MODELS` désigne un dossier qui contient
//! `phonon_2/phonon-2.bps.tar.zst`, et ils échouent sans lui.
//!
//! La parité (`parity`) ne vaut que dans le build de `scripts/mlx-parite.sh`,
//! avec les noyaux Metal du Swift, comme celle de Parakeet (voir l'en-tête
//! de `parakeet/tests.rs`) ; `shipped_smoke` est la garde du build livré
//! (`scripts/mlx-parite.sh --livre`). Les autres essais fabriquent tout ici :
//! archives, conteneurs et poids synthétiques, sans les vrais poids.

use super::archive::read_members;
use super::archive::tests::{archive_file, tar_zst, Lcg};
use super::container::{self, FiveValueLayer, FORMAT};
use super::*;
use crate::compute::mlx::budget;
use crate::compute::mlx::Held;
use crate::stt::mlx::nn::{Linear, FIVE_VALUE_GROUP_SIZE};
use crate::stt::mlx::parakeet::model::remap_key;
use crate::stt::mlx::parakeet::tests::{
    model_dir_with, parity, serial, shipped_smoke, tiny_config, tiny_keys, Subject,
};
use crate::stt::mlx::{BUDGET_KEY, MEMORY_MARGIN};
use half::f16;
use mlx_rs::{Array, Dtype};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const ID: &str = "phonon_2";

/// Phonon-2 : l'archive du dossier, lue comme l'app.
fn phonon() -> Subject {
    Subject {
        id: ID,
        dir: model_dir_with(ID, ARCHIVE),
        open: load,
        quantization: format!("{FORMAT}, groupes de {FIVE_VALUE_GROUP_SIZE}"),
    }
}

/// P2 : la parité avec la fixture du Swift (`tests/fixtures/mlx/phonon_2.json`),
/// étage par étage, dans le build de `scripts/mlx-parite.sh` (noyaux du
/// Swift) : mel, encodeur, chaque pas du décodeur, jetons et texte.
mod parity {
    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS) et le build de scripts/mlx-parite.sh"]
    fn phonon_2() {
        super::parity(&super::phonon());
    }
}

/// La garde de fumée du build livré (AOT, le mlx.metallib de mlx-sys), par
/// `scripts/mlx-parite.sh --livre`, comme celle de Parakeet (R62) : le mel à
/// l'octet (forme, moyenne, moyenne absolue), le nombre de pas à 10 % près
/// de la fixture (arrondi vers le bas), et le texte de la fixture sur les
/// entrées de la langue du modèle : Phonon-2 est réentraîné pour l'anglais
/// (le français y est « moins sûr », catalogue), donc l'anglais et le
/// silence. Ce n'est pas la preuve de parité, qui reste le build JIT.
mod shipped_smoke {
    const TEXT: &[&str] = &["anglais", "silence"];

    #[test]
    #[ignore = "demande les poids (DYSFLOW_MODELS), dans le build livré (scripts/mlx-parite.sh --livre)"]
    fn phonon_2() {
        super::shipped_smoke(&super::phonon(), TEXT);
    }
}

// ---------- fumée (PhononContainerTests.swift) ----------

/// `testTheRealArchiveLoadsAndTranscribesSilence` : la vraie archive se
/// charge et rend un texte d'une seconde de silence.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_real_archive_loads_and_transcribes_silence() {
    let _serial = serial();
    let recognizer = PhononMlx::new();
    recognizer.load(&model_dir_with(ID, ARCHIVE)).unwrap();
    recognizer
        .transcribe(&[0.0; 16_000], "en", &Cancel::never())
        .unwrap();
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
}

/// La vraie archive, lue par ruzstd et tar, rend les membres que décrit son
/// `bps_manifest.json` (tailles et SHA-256, révision 7e153e40) : la lecture
/// Rust est celle du `tar` de macOS, à l'octet.
#[test]
#[ignore = "demande les poids (DYSFLOW_MODELS)"]
fn the_real_archive_gives_the_members_of_its_manifest() {
    let archive = model_dir_with(ID, ARCHIVE).join(ARCHIVE);
    let [config, model] = read_members(&archive, ["config.json", "model.fermion"]).unwrap();
    assert_eq!(
        (config.len(), hex(&Sha256::digest(&config))),
        (
            277_493,
            "d0daad3b2a182893844f4abdc11e4f5b7083f7d42c8ad7e8203f71559785a31b".to_string()
        )
    );
    assert_eq!(
        (model.len(), hex(&Sha256::digest(&model))),
        (
            177_438_361,
            "4b6bfa3a12cc3c4e0a54f2ab3ec4ca7a842b09e5c7ecfc8e7ca0ac6cc8c11468".to_string()
        )
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Écrit [rows, columns] au format `five_value` (`fiveValueBlob` du Swift) :
/// trits (chiffre 1 = zéro, lignes complétées à l'octet par le chiffre 2,
/// que le lecteur doit ignorer), un bit par non-nul (1 = hi), puis lo et hi
/// en float16.
fn five_value_blob(signs: &[Vec<i32>], is_hi: &[Vec<bool>], lo: &[f16], hi: &[f16]) -> Vec<u8> {
    let columns = signs[0].len();
    let row_bytes = columns.div_ceil(5);
    let (mut data, mut bits) = (Vec::new(), Vec::new());
    for (r, row) in signs.iter().enumerate() {
        for b in 0..row_bytes {
            let mut byte = 0;
            for k in (0..5).rev() {
                let c = b * 5 + k;
                byte = byte * 3 + if c < columns { row[c] + 1 } else { 2 };
            }
            data.push(byte as u8);
        }
        bits.extend((0..columns).filter(|&c| row[c] != 0).map(|c| is_hi[r][c]));
    }
    let mut packed = vec![0u8; bits.len().div_ceil(8)];
    for (i, _) in bits.iter().enumerate().filter(|(_, &bit)| bit) {
        packed[i / 8] |= 1 << (i % 8);
    }
    data.extend(packed);
    for value in lo.iter().chain(hi) {
        data.extend(value.to_le_bytes());
    }
    data
}

/// `testFiveValueRowsRoundTripThroughTwoPlanes` : une matrice à cinq valeurs
/// se relit à l'identique en deux plans 2 bits, et la couche qui en sort
/// calcule le même produit que la matrice dense.
#[test]
fn five_value_rows_round_trip_through_two_planes() {
    let (rows, columns) = (3usize, 256usize);
    let mut rng = Lcg(16);
    let signs: Vec<Vec<i32>> = (0..rows)
        .map(|_| (0..columns).map(|_| (rng.next() % 3) as i32 - 1).collect())
        .collect();
    let is_hi: Vec<Vec<bool>> = (0..rows)
        .map(|_| (0..columns).map(|_| rng.next() % 2 == 1).collect())
        .collect();
    let lo = [0.25, 0.5, 0.125].map(f16::from_f32);
    let hi = [1.5, 2.0, 0.75].map(f16::from_f32);
    let blob = five_value_blob(&signs, &is_hi, &lo, &hi);
    let expected: Vec<f32> = (0..rows)
        .flat_map(|r| {
            let (signs, is_hi) = (&signs[r], &is_hi[r]);
            (0..columns)
                .map(move |c| signs[c] as f32 * f32::from(if is_hi[c] { hi[r] } else { lo[r] }))
        })
        .collect();
    let x: Vec<f32> = (0..2 * columns).map(|_| rng.unit()).collect();

    let held = Held::load(|| Ok(())).unwrap();
    let dense_expected = expected.clone();
    let (dense, difference, scale) = held
        .with(move |_| {
            let planes =
                container::planes(&blob, rows, columns, "essai").map_err(anyhow::Error::msg)?;
            let layer = FiveValueLayer::new(planes)?;
            let dense = layer.dense()?.as_dtype(Dtype::Float32)?;
            dense.eval()?;
            let dense = dense.as_slice::<f32>().to_vec();

            let path = PathBuf::from("essai");
            let arrays = Vec::from(layer.into_arrays("l"));
            let mut w = Weights::from_arrays("essai", &path, arrays, Dtype::Float16)?;
            let linear = Linear::load(&mut w, "l", columns as i32, rows as i32, false)?;
            w.finish()?;
            let x = Array::from_slice(&x, &[2, columns as i32]);
            let reference = x.matmul(
                Array::from_slice(&dense_expected, &[rows as i32, columns as i32]).transpose()?,
            )?;
            let y = linear.forward(&x)?.as_dtype(Dtype::Float32)?;
            let difference = mlx_rs::ops::abs(y.subtract(&reference)?)?.max(None)?;
            let scale = mlx_rs::ops::abs(&reference)?.max(None)?;
            mlx_rs::transforms::eval([&difference, &scale])?;
            Ok((
                dense,
                difference.try_item_exact::<f32>()?,
                scale.try_item_exact::<f32>()?,
            ))
        })
        .unwrap();
    assert_eq!(dense, expected, "plans relus à l'identique");
    // Le produit 2 bits de MLX calcule s·Σqx − s·Σx (échelles en float16) :
    // un écart de l'ordre de 1e-3 relatif, pas une erreur de poids.
    assert!(difference < 1e-2 * scale, "{difference} pour {scale}");
}

/// `testABlobOfTheWrongSizeIsRefused`.
#[test]
fn a_blob_of_the_wrong_size_is_refused() {
    let mut blob = five_value_blob(
        &[vec![1; 128]],
        &[vec![false; 128]],
        &[f16::ONE],
        &[f16::from_f32(2.0)],
    );
    container::planes(&blob, 1, 128, "essai").unwrap();
    blob.push(0);
    let err = container::planes(&blob, 1, 128, "essai").err().unwrap();
    assert!(err.contains("essai"), "{err}");
}

/// Un octet de trits au-delà de 242 n'est pas cinq trits : refusé (le Swift
/// le lirait hors de sa table).
#[test]
fn a_byte_beyond_five_trits_is_refused() {
    let mut blob = five_value_blob(
        &[vec![0; 128]],
        &[vec![false; 128]],
        &[f16::ONE],
        &[f16::ONE],
    );
    blob[3] = 243;
    let err = container::planes(&blob, 1, 128, "essai").err().unwrap();
    assert!(err.contains("243") && err.contains("essai"), "{err}");
}

/// `testIntSixRowsAreScaled` : deux lignes de quatre, q = −32, −1, 0, 31 puis
/// 1, 2, 3, 4 (décalés de 32), échelles 0,5 et 2.
#[test]
fn int_six_rows_are_scaled() {
    let q: [i32; 8] = [-32, -1, 0, 31, 1, 2, 3, 4];
    let mut data = Vec::new();
    for group in q.chunks(4) {
        let packed = group
            .iter()
            .enumerate()
            .fold(0u32, |p, (k, &v)| p | ((v + 32) as u32) << (6 * k));
        data.extend([packed as u8, (packed >> 8) as u8, (packed >> 16) as u8]);
    }
    for scale in [0.5f32, 2.0] {
        data.extend(f16::from_f32(scale).to_le_bytes());
    }
    assert_eq!(
        container::int_six(&data, &[2, 4]).unwrap(),
        vec![-16.0, -0.5, 0.0, 15.5, 2.0, 4.0, 6.0, 8.0]
    );
    assert!(container::int_six(&data[..data.len() - 1], &[2, 4]).is_err());
}

/// `testTransformersNamesMapToTheSwiftGraph` : les noms de transformers
/// deviennent ceux du graphe (ceux de mlx-community, puis `remap_key`).
#[test]
fn transformers_names_map_to_the_swift_graph() {
    let cases = [
        (
            "encoder.subsampling.layers.0.weight",
            "encoder.pre_encode.conv0.weight",
        ),
        (
            "encoder.subsampling.layers.2.bias",
            "encoder.pre_encode.depthwise_layers.0.bias",
        ),
        (
            "encoder.subsampling.layers.3.weight",
            "encoder.pre_encode.pointwise_layers.0.weight",
        ),
        (
            "encoder.subsampling.layers.5.weight",
            "encoder.pre_encode.depthwise_layers.1.weight",
        ),
        (
            "encoder.subsampling.layers.6.weight",
            "encoder.pre_encode.pointwise_layers.1.weight",
        ),
        (
            "encoder.subsampling.linear.weight",
            "encoder.pre_encode.out.weight",
        ),
        (
            "encoder.layers.3.self_attn.q_proj",
            "encoder.layers.3.self_attn.linear_q",
        ),
        (
            "encoder.layers.3.self_attn.o_proj",
            "encoder.layers.3.self_attn.linear_out",
        ),
        (
            "encoder.layers.3.self_attn.relative_k_proj",
            "encoder.layers.3.self_attn.linear_pos",
        ),
        (
            "encoder.layers.3.self_attn.bias_u",
            "encoder.layers.3.self_attn.posBiasU",
        ),
        (
            "encoder.layers.3.conv.norm.running_var",
            "encoder.layers.3.conv.batch_norm.running_var",
        ),
        (
            "encoder.layers.3.conv.pointwise_conv1",
            "encoder.layers.3.conv.pointwise_conv1",
        ),
        (
            "encoder.layers.3.feed_forward2.linear1",
            "encoder.layers.3.feed_forward2.linear1",
        ),
        (
            "decoder.embedding.weight",
            "decoder.prediction.embed.weight",
        ),
        (
            "decoder.lstm.weight_ih_l1",
            "decoder.prediction.dec_rnn.lstm.1.Wx",
        ),
        (
            "decoder.lstm.weight_hh_l0",
            "decoder.prediction.dec_rnn.lstm.0.Wh",
        ),
        ("decoder.decoder_projector.bias", "joint.pred.bias"),
        ("encoder_projector.weight", "joint.enc.weight"),
        ("joint.head.weight", "joint.joint_net.weight"),
    ];
    for (source, target) in cases {
        assert_eq!(container::graph_key(source), target, "{source}");
    }
}

// ---------- un Phonon synthétique (sans poids) ----------

/// Un enregistrement du conteneur : nom, sorte, forme, octets.
type Record = (String, &'static str, Vec<usize>, Vec<u8>);

/// `model.fermion` : l'en-tête JSON (taille sur 8 octets), puis les octets.
fn container_bytes(records: &[Record]) -> Vec<u8> {
    let index: Vec<_> = records
        .iter()
        .map(|(n, k, shape, blob)| serde_json::json!({"n": n, "k": k, "shape": shape, "b": blob.len()}))
        .collect();
    let header = serde_json::json!({"format": FORMAT, "index": index}).to_string();
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend(header.as_bytes());
    for (_, _, _, blob) in records {
        bytes.extend(blob);
    }
    bytes
}

fn fp16_zeros(shape: &[usize]) -> Vec<u8> {
    vec![0; 2 * shape.iter().product::<usize>()]
}

/// `int6`, q = 0 partout (32 décalé), échelle 1.
fn int6_zeros(shape: &[usize]) -> Vec<u8> {
    let total: usize = shape.iter().product();
    let mut blob = [0x20u8, 0x08, 0x82].repeat(total.div_ceil(4));
    for _ in 0..shape[0] {
        blob.extend(f16::ONE.to_le_bytes());
    }
    blob
}

/// `five_value`, tous les poids nuls (cinq chiffres 1 par octet : 121).
fn five_value_zeros(rows: usize, columns: usize) -> Vec<u8> {
    let mut blob = vec![121u8; rows * columns.div_ceil(5)];
    blob.extend(vec![0u8; 4 * rows]);
    blob
}

/// Le graphe de `tiny_config` (Parakeet), écrit comme Fermion : noms de
/// transformers là où ils diffèrent (LSTM), dispositions de PyTorch (canaux
/// avant le noyau), une FFN à cinq valeurs, une projection en `int6`, deux
/// biais de LSTM à sommer et un compteur de BatchNorm ignoré.
fn tiny_records() -> Vec<Record> {
    let mut records = Vec::new();
    for (key, shape) in tiny_keys() {
        let Some(key) = remap_key(&key) else { continue };
        let record: Record = match key.as_str() {
            "encoder.layers.0.feed_forward1.linear2.weight" => (
                "encoder.layers.0.feed_forward1.linear2".into(),
                "five_value",
                shape.clone(),
                five_value_zeros(shape[0], shape[1]),
            ),
            "joint.enc.weight" => (key, "int6", shape.clone(), int6_zeros(&shape)),
            "decoder.prediction.dec_rnn.lstm.0.Wx" => (
                "decoder.lstm.weight_ih_l0".into(),
                "fp16",
                shape.clone(),
                fp16_zeros(&shape),
            ),
            "decoder.prediction.dec_rnn.lstm.0.bias" => {
                for kind in ["ih", "hh"] {
                    records.push((
                        format!("decoder.lstm.bias_{kind}_l0"),
                        "fp16",
                        shape.clone(),
                        fp16_zeros(&shape),
                    ));
                }
                continue;
            }
            _ => {
                let torch = match shape[..] {
                    [o, k, i] => vec![o, i, k],
                    [o, kh, kw, i] => vec![o, i, kh, kw],
                    _ => shape.clone(),
                };
                let blob = fp16_zeros(&torch);
                (key, "fp16", torch, blob)
            }
        };
        records.push(record);
    }
    records.push((
        "encoder.layers.0.conv.norm.num_batches_tracked".into(),
        "fp16",
        vec![],
        vec![0, 0],
    ));
    records
}

/// Un dossier `phonon_2` synthétique : l'archive avec `config.json`
/// (`tiny_config`, `-Infinity` compris) et `model.fermion`.
fn synthetic(name: &str, container: &[u8]) -> PathBuf {
    let config = tiny_config(false).to_string().replace("null", "-Infinity");
    let archive = tar_zst(&[
        (b"config.json", config.as_bytes(), config.len() as u64),
        (b"model.fermion", container, container.len() as u64),
    ]);
    archive_file(name, &archive).parent().unwrap().to_path_buf()
}

fn load_error(dir: &Path) -> String {
    let _serial = serial();
    let err = PhononMlx::new().load(dir).unwrap_err();
    assert_eq!(budget::of(BUDGET_KEY), None);
    format!("{err:#}")
}

/// Le dossier se charge, le besoin `"dictee"` (l'archive plus 2 Gio) est
/// posé puis retiré, un silence se transcrit, et moins de 0,1 s ne lance
/// pas le modèle.
#[test]
fn a_complete_synthetic_phonon_loads_and_transcribes() {
    let dir = synthetic("complet", &container_bytes(&tiny_records()));
    let _serial = serial();
    let recognizer = PhononMlx::new();
    recognizer.load(&dir).unwrap();
    let archive = std::fs::metadata(dir.join(ARCHIVE)).unwrap().len() as usize;
    assert_eq!(
        budget::of(BUDGET_KEY).map(|need| need.memory),
        Some(archive + MEMORY_MARGIN)
    );
    let text = recognizer
        .transcribe(&[0.0; 4_000], "en", &Cancel::never())
        .unwrap();
    assert!(text.len() < 1_000);
    assert_eq!(
        recognizer
            .transcribe(
                &[0.0; crate::stt::mlx::parakeet::MIN_SAMPLES - 1],
                "en",
                &Cancel::never()
            )
            .unwrap(),
        ""
    );
    recognizer.release();
    assert_eq!(budget::of(BUDGET_KEY), None);
}

#[test]
fn a_transcription_without_a_model_is_an_error_that_names_phonon() {
    let err = PhononMlx::new()
        .transcribe(&[0.0; 16_000], "en", &Cancel::never())
        .unwrap_err();
    assert!(format!("{err}").contains("Phonon-2"), "{err}");
}

#[test]
fn a_missing_archive_is_an_error_that_names_it() {
    let dir = crate::paths::test_scratch("stt-mlx-phonon-sans-archive");
    let err = load_error(&dir);
    assert!(err.contains(ARCHIVE), "{err}");
}

#[test]
fn a_truncated_container_is_an_error_that_names_the_archive() {
    let mut bytes = container_bytes(&tiny_records());
    bytes.truncate(bytes.len() - 10);
    let dir = synthetic("conteneur-coupe", &bytes);
    let err = load_error(&dir);
    assert!(
        err.contains(&dir.join(ARCHIVE).display().to_string()),
        "{err}"
    );
    assert!(err.contains("Phonon"), "{err}");
}

#[test]
fn a_missing_key_is_an_error_that_names_it() {
    let records: Vec<Record> = tiny_records()
        .into_iter()
        .filter(|(n, ..)| n != "joint.pred.weight")
        .collect();
    let err = load_error(&synthetic("cle-absente", &container_bytes(&records)));
    assert!(err.contains("joint.pred.weight"), "{err}");
}

#[test]
fn a_wrong_shape_is_an_error_that_names_the_key_and_the_archive() {
    let records: Vec<Record> = tiny_records()
        .into_iter()
        .map(|(n, k, shape, blob)| {
            if n == "joint.pred.weight" {
                (n, k, vec![16, 8], fp16_zeros(&[16, 8]))
            } else {
                (n, k, shape, blob)
            }
        })
        .collect();
    let dir = synthetic("mauvaise-forme", &container_bytes(&records));
    let err = load_error(&dir);
    assert!(err.contains("joint.pred.weight"), "{err}");
    assert!(err.contains("[16, 8]") && err.contains("[16, 16]"), "{err}");
    assert!(
        err.contains(&dir.join(ARCHIVE).display().to_string()),
        "{err}"
    );
}

#[test]
fn an_extra_key_is_an_error_that_names_it() {
    let mut records = tiny_records();
    records.push(("encoder.de.trop".into(), "fp16", vec![2], fp16_zeros(&[2])));
    let err = load_error(&synthetic("cle-en-trop", &container_bytes(&records)));
    assert!(err.contains("encoder.de.trop"), "{err}");
}

/// Un seul des deux biais d'une couche du LSTM : refusé.
#[test]
fn a_lone_lstm_bias_is_an_error() {
    let records: Vec<Record> = tiny_records()
        .into_iter()
        .filter(|(n, ..)| n != "decoder.lstm.bias_hh_l0")
        .collect();
    let err = load_error(&synthetic("biais-seul", &container_bytes(&records)));
    assert!(err.contains("decoder.prediction.dec_rnn.lstm.0"), "{err}");
}

/// Une clé du graphe donnée deux fois : une erreur qui la nomme, pas la
/// dernière qui l'emporte.
#[test]
fn a_key_given_twice_is_an_error_that_names_it() {
    let mut records = tiny_records();
    let twice = records
        .iter()
        .find(|(n, ..)| n == "joint.pred.weight")
        .cloned()
        .unwrap();
    records.push(twice);
    let err = load_error(&synthetic("cle-deux-fois", &container_bytes(&records)));
    assert!(err.contains("joint.pred.weight"), "{err}");
    assert!(err.contains("twice") || err.contains("deux fois"), "{err}");
}

/// Un biais du LSTM donné deux fois : une erreur qui le nomme, avec
/// l'archive.
#[test]
fn an_lstm_bias_given_twice_is_an_error_that_names_it() {
    let mut records = tiny_records();
    let twice = records
        .iter()
        .find(|(n, ..)| n == "decoder.lstm.bias_ih_l0")
        .cloned()
        .unwrap();
    records.push(twice);
    let dir = synthetic("biais-deux-fois", &container_bytes(&records));
    let err = load_error(&dir);
    assert!(err.contains("decoder.lstm.bias_ih_l0"), "{err}");
    assert!(err.contains("twice") || err.contains("deux fois"), "{err}");
    assert!(
        err.contains(&dir.join(ARCHIVE).display().to_string()),
        "{err}"
    );
}

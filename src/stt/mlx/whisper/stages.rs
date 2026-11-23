//! La comparaison étage par étage (méthode R54 de la tâche 14) : chaque
//! étage Rust relancé sur l'entrée exacte du Swift, écrite par
//! `macos/Tests/WhisperStagesTests.swift`, et comparé élément par élément.
//! Nomme tous les étages qui diffèrent, pas seulement le premier.
//!
//!   TEST_RUNNER_DYSFLOW_STT_MODELS=… TEST_RUNNER_DYSFLOW_STAGES_OUT=… \
//!     ./scripts/test-macos.sh WhisperStagesTests
//!   DYSFLOW_MODELS=… DYSFLOW_WHISPER_STAGES=… \
//!     ./scripts/mlx-parite.sh stt::mlx::whisper::stages 1
//!
//! Dans le build de parité (noyaux du Swift), tout doit être identique ; dans
//! le build livré (`--livre`), les étages qui diffèrent sont ceux des noyaux.

use anyhow::{anyhow, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{nn, ops, Array, Dtype};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::audio;
use super::layers::LayerCache;
use super::model::Whisper;
use crate::compute::mlx::Held;

struct Checker {
    id: String,
    differing: Vec<String>,
}

impl Checker {
    /// `got` contre `want` : même forme, même type, mêmes valeurs à l'octet.
    fn check(&mut self, name: &str, got: &Array, want: &Array) -> Result<()> {
        if got.shape() != want.shape() || got.dtype() != want.dtype() {
            self.differing.push(format!(
                "{name} : {:?} {:?} contre {:?} {:?}",
                got.shape(),
                got.dtype(),
                want.shape(),
                want.dtype()
            ));
            return Ok(());
        }
        let (g, w) = (
            got.as_dtype(Dtype::Float32)?,
            want.as_dtype(Dtype::Float32)?,
        );
        let diff = ops::abs(g.subtract(&w)?)?;
        let differ = g.ne(&w)?.as_dtype(Dtype::Float32)?.sum(None)?;
        let max = diff.max(None)?;
        mlx_rs::transforms::eval([&differ, &max])?;
        let (differ, max): (f32, f32) = (differ.try_item_exact()?, max.try_item_exact()?);
        eprintln!(
            "{} {name:14} {:?} : {differ} éléments différents, max|Δ| {max:.3e}",
            self.id,
            got.shape()
        );
        if differ > 0.0 {
            self.differing
                .push(format!("{name} ({differ} éléments, max|Δ| {max:.3e})"));
        }
        Ok(())
    }
}

/// Les étages de `id` qui diffèrent du Swift (vide : tous identiques).
fn compare(id: &str) -> Vec<String> {
    let base =
        PathBuf::from(std::env::var_os("DYSFLOW_WHISPER_STAGES").expect("DYSFLOW_WHISPER_STAGES"));
    let models = PathBuf::from(std::env::var_os("DYSFLOW_MODELS").expect("DYSFLOW_MODELS"));
    let (dir, file, shown) = (
        models.join(id),
        base.join(format!("{id}-etages.safetensors")),
        id.to_string(),
    );
    assert!(file.is_file(), "{} absent", file.display());
    let held = Held::load(move || Whisper::load(&dir)).unwrap();
    let differing = held
        .with(move |whisper| {
            let mut stage = Checker {
                id: shown,
                differing: Vec::new(),
            };
            let d: HashMap<String, Array> = Array::load_safetensors(&file)?;
            let get = |k: &str| d.get(k).ok_or_else(|| anyhow!("étage {k} absent"));
            let wav = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("macos/Tests/Fixtures/dictee-charge.wav");
            let samples = crate::stt::file::read_wav(&wav)?;

            stage.check("window", &audio::hann_window()?, get("window")?)?;
            let n_mels = whisper.config.num_mel_bins;
            stage.check(
                "features",
                &audio::features(&samples, n_mels)?,
                get("features")?,
            )?;

            let e = &whisper.encoder;
            let features = get("features")?;
            stage.check(
                "e_conv1",
                &nn::gelu(e.conv1.forward(features)?)?,
                get("e_conv1")?,
            )?;
            stage.check(
                "e_conv2",
                &nn::gelu(e.conv2.forward(get("e_conv1")?)?)?,
                get("e_conv2")?,
            )?;
            stage.check("e_pos", &e.with_positions(get("e_conv2")?)?, get("e_pos")?)?;
            let l = &e.layers[0];
            let pos = get("e_pos")?;
            stage.check(
                "l0_ln1",
                &l.self_attn_layer_norm.forward(pos)?,
                get("l0_ln1")?,
            )?;
            let ln1 = get("l0_ln1")?;
            stage.check(
                "l0_attn",
                &l.self_attn.forward(ln1, ln1, None, None)?.0,
                get("l0_attn")?,
            )?;
            stage.check("l0_res1", &pos.add(get("l0_attn")?)?, get("l0_res1")?)?;
            stage.check(
                "l0_ln2",
                &l.final_layer_norm.forward(get("l0_res1")?)?,
                get("l0_ln2")?,
            )?;
            stage.check("l0_fc1", &l.fc1.forward(get("l0_ln2")?)?, get("l0_fc1")?)?;
            stage.check("l0_gelu", &nn::gelu(get("l0_fc1")?)?, get("l0_gelu")?)?;
            stage.check("l0_fc2", &l.fc2.forward(get("l0_gelu")?)?, get("l0_fc2")?)?;
            stage.check("l0", &get("l0_res1")?.add(get("l0_fc2")?)?, get("l0")?)?;
            stage.check("l0_direct", &l.forward(pos)?, get("l0_direct")?)?;
            stage.check("encoder", &e.forward(features)?, get("encoder")?)?;

            let dec = &whisper.decoder;
            let encoded = get("encoder")?;
            let mut caches: Vec<LayerCache> = (0..whisper.config.decoder_layers)
                .map(|_| LayerCache::default())
                .collect();
            let prompt = get("d_prompt")?;
            let hidden = dec.forward(prompt, 0, encoded, &mut caches)?;
            stage.check("d_hidden", &hidden, get("d_hidden")?)?;
            let last = get("d_hidden")?.try_index((0, -1))?;
            stage.check("d_logits", &dec.project(&last)?, get("d_logits")?)?;
            let token = get("d_step_token")?.reshape(&[1, 1])?;
            let prompt_len = prompt.shape()[1];
            let step = dec.forward(&token, prompt_len, encoded, &mut caches)?;
            stage.check("d_step_hidden", &step, get("d_step_hidden")?)?;
            let last = get("d_step_hidden")?.try_index((0, -1))?;
            stage.check("d_step_logits", &dec.project(&last)?, get("d_step_logits")?)?;
            Ok(stage.differing)
        })
        .unwrap();
    differing.into_iter().map(|d| format!("{id} {d}")).collect()
}

#[test]
#[ignore = "demande les poids et les étages du Swift, dans le build de scripts/mlx-parite.sh"]
fn every_stage_matches_the_swift() {
    let differing: Vec<String> = [
        "whisper_tiny_4bit",
        "whisper_small_4bit",
        "whisper_large_v3_turbo_4bit",
    ]
    .into_iter()
    .flat_map(compare)
    .collect();
    assert!(differing.is_empty(), "{}", differing.join(" ; "));
}

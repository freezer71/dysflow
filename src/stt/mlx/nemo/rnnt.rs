//! Le réseau de prédiction (LSTM empilés) et le réseau joint d'un
//! transducteur NeMo. Port de `NemoRNNTLayers.swift`.

use anyhow::{bail, ensure, Result};
use mlx_rs::ops::indexing::TryIndexOp;
use mlx_rs::{ops, Array};

use super::config::{JointConfig, PredictConfig};
use crate::stt::mlx::nn::{Embedding, Linear, Lstm};
use crate::stt::mlx::weights::Weights;

/// L'état des LSTM : `[couches, lot, caché]`, états cachés et cellules.
#[derive(Clone)]
pub struct LstmState {
    pub hidden: Array,
    pub cell: Array,
}

/// `NemoStackedLSTM` : des LSTM l'un sur l'autre, lot en premier.
pub struct StackedLstm {
    layers: Vec<Lstm>,
}

impl StackedLstm {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        input: i32,
        hidden: i32,
        layers: i32,
    ) -> Result<Self> {
        ensure!(layers > 0, "{prefix} : {layers} couches de LSTM");
        let layers = (0..layers)
            .map(|i| {
                Lstm::load(
                    w,
                    &format!("{prefix}.lstm.{i}"),
                    if i == 0 { input } else { hidden },
                    hidden,
                    true,
                )
            })
            .collect::<Result<_>>()?;
        Ok(Self { layers })
    }

    pub fn num_layers(&self) -> i32 {
        self.layers.len() as i32
    }

    pub fn hidden_size(&self) -> i32 {
        self.layers[0].hidden_size
    }

    /// `x` : `[lot, temps, entrées]`. Un état dont la première dimension
    /// n'est pas le nombre de couches est ignoré, comme en Swift.
    pub fn forward(&self, x: &Array, state: Option<&LstmState>) -> Result<(Array, LstmState)> {
        let n = self.layers.len();
        let split = |a: &Array| -> Result<Option<Vec<Array>>> {
            if a.shape().first() != Some(&(n as i32)) {
                return Ok(None);
            }
            Ok(Some(
                a.split_equal(n as i32, 0)?
                    .into_iter()
                    .map(|p| p.squeeze_axes(&[0]))
                    .collect::<Result<_, _>>()?,
            ))
        };
        let hidden = match state {
            Some(s) => split(&s.hidden)?,
            None => None,
        };
        let cell = match state {
            Some(s) => split(&s.cell)?,
            None => None,
        };
        let mut output = x.clone();
        let (mut next_hidden, mut next_cell) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for (i, layer) in self.layers.iter().enumerate() {
            let h = hidden.as_ref().map(|h| h[i].clone());
            let c = cell.as_ref().map(|c| c[i].clone());
            let (all_h, all_c) = layer.forward(&output, h, c)?;
            let last = all_h.shape()[1] - 1;
            next_hidden.push(all_h.try_index((.., last, ..))?);
            next_cell.push(all_c.try_index((.., last, ..))?);
            output = all_h;
        }
        Ok((
            output,
            LstmState {
                hidden: ops::stack(&next_hidden, 0)?,
                cell: ops::stack(&next_cell, 0)?,
            },
        ))
    }
}

/// `NemoPredictNetwork` : plongement du dernier jeton, puis les LSTM.
pub struct PredictNetwork {
    pub embed: Embedding,
    pub dec_rnn: StackedLstm,
    pub pred_hidden: i32,
}

impl PredictNetwork {
    /// Clés `decoder.prediction.embed` et `decoder.prediction.dec_rnn`.
    pub fn load(w: &mut Weights, prefix: &str, args: &PredictConfig) -> Result<Self> {
        let count = if args.blank_as_pad {
            args.vocab_size + 1
        } else {
            args.vocab_size
        };
        let hidden = args.prednet.pred_hidden;
        Ok(Self {
            embed: Embedding::load(w, &format!("{prefix}.prediction.embed"), count, hidden)?,
            dec_rnn: StackedLstm::load(
                w,
                &format!("{prefix}.prediction.dec_rnn"),
                hidden,
                args.prednet.rnn_hidden_size.unwrap_or(hidden),
                args.prednet.pred_rnn_layers,
            )?,
            pred_hidden: hidden,
        })
    }

    /// `callAsFunction(_:state:)` : le plongement de `token` (`[1, 1]`,
    /// `int32`), ou sans jeton des zéros `f32` `[lot, 1, pred_hidden]` (le
    /// lot pris de l'état, 1 sans état), puis les LSTM. Le RNN-T de Nemotron.
    pub fn predict(
        &self,
        token: Option<&Array>,
        state: Option<&LstmState>,
    ) -> Result<(Array, LstmState)> {
        let embedded = match token {
            Some(token) => self.embed.forward(token)?,
            None => {
                let batch = state
                    .and_then(|s| s.hidden.shape().get(1).copied())
                    .unwrap_or(1);
                Array::zeros::<f32>(&[batch, 1, self.pred_hidden])?
            }
        };
        self.dec_rnn.forward(&embedded, state)
    }

    /// Un pas : le plongement de `token` (`[1, 1]`, `int32`), mis à zéro si
    /// c'est le blanc, puis les LSTM. Comme l'étape compilée du Swift.
    pub fn step(
        &self,
        token: &Array,
        blank: &Array,
        state: &LstmState,
    ) -> Result<(Array, LstmState)> {
        let embedded = self.embed.forward(token)?;
        let is_blank = token.eq(blank)?.expand_dims(2)?;
        let masked = ops::select(&is_blank, ops::zeros_like(&embedded)?, &embedded)?;
        self.dec_rnn.forward(&masked, Some(state))
    }
}

/// L'activation du réseau joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Activation {
    Relu,
    Sigmoid,
    Tanh,
}

/// `NemoJointNetwork` : encodeur et prédiction projetés, sommés, activés,
/// puis projetés sur les jetons (blanc compris) et les durées.
pub struct JointNetwork {
    pred: Linear,
    enc: Linear,
    joint_net: Linear,
    activation: Activation,
    pub num_classes: i32,
}

impl JointNetwork {
    pub fn load(w: &mut Weights, prefix: &str, args: &JointConfig) -> Result<Self> {
        let net = &args.jointnet;
        let num_classes = args.num_classes + 1 + args.num_extra_outputs;
        let activation = match net.activation.to_lowercase().as_str() {
            "relu" => Activation::Relu,
            "sigmoid" => Activation::Sigmoid,
            // Le Swift prend tanh pour toute autre valeur.
            _ => Activation::Tanh,
        };
        Ok(Self {
            pred: Linear::load(
                w,
                &format!("{prefix}.pred"),
                net.pred_hidden,
                net.joint_hidden,
                true,
            )?,
            enc: Linear::load(
                w,
                &format!("{prefix}.enc"),
                net.encoder_hidden,
                net.joint_hidden,
                true,
            )?,
            joint_net: Linear::load(
                w,
                &format!("{prefix}.joint_net"),
                net.joint_hidden,
                num_classes,
                true,
            )?,
            activation,
            num_classes,
        })
    }

    /// `enc_out` : `[lot, T, enc]` ; `pred_out` : `[lot, U, pred]` ; rend
    /// `[lot, T, U, classes]`.
    pub fn forward(&self, enc_out: &Array, pred_out: &Array) -> Result<Array> {
        let x = self
            .enc
            .forward(enc_out)?
            .expand_dims(2)?
            .add(self.pred.forward(pred_out)?.expand_dims(1)?)?;
        let x = match self.activation {
            Activation::Relu => mlx_rs::nn::relu(&x)?,
            Activation::Sigmoid => ops::sigmoid(&x)?,
            Activation::Tanh => ops::tanh(&x)?,
        };
        self.joint_net.forward(&x)
    }
}

/// Le choix d'un pas TDT dans les sorties du réseau joint `[1, 1, 1, …]` :
/// le jeton (blanc compris, `blank + 1` premières) et l'indice de durée (le
/// reste), chacun par `argmax`.
pub fn tdt_decisions(joint: &Array, blank: i32) -> Result<Array> {
    let classes = joint.shape().last().copied().unwrap_or(0);
    if classes <= blank + 1 {
        bail!("réseau joint : {classes} sorties pour un blanc {blank}, aucune durée");
    }
    let logits = joint.try_index((0, 0, 0))?;
    let token = ops::indexing::argmax_axis(logits.try_index(..blank + 1)?, -1, None)?
        .as_dtype(mlx_rs::Dtype::Int32)?;
    let duration = ops::indexing::argmax_axis(logits.try_index(blank + 1..)?, -1, None)?
        .as_dtype(mlx_rs::Dtype::Int32)?;
    Ok(ops::stack(&[token, duration], 0)?)
}

//! Les réglages du réseau de prédiction et du réseau joint d'un transducteur
//! NeMo, tels que `config.json` les donne. Port de `NemoRNNTConfig.swift`.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PredictNetworkConfig {
    pub pred_hidden: i32,
    pub pred_rnn_layers: i32,
    #[serde(default)]
    pub rnn_hidden_size: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PredictConfig {
    pub blank_as_pad: bool,
    pub vocab_size: i32,
    pub prednet: PredictNetworkConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JointNetworkConfig {
    pub joint_hidden: i32,
    pub activation: String,
    pub encoder_hidden: i32,
    pub pred_hidden: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JointConfig {
    pub num_classes: i32,
    pub vocabulary: Vec<String>,
    pub jointnet: JointNetworkConfig,
    #[serde(default)]
    pub num_extra_outputs: i32,
}

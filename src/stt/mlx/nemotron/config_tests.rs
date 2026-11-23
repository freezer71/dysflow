//! Les essais de la lecture de `config.json` de Nemotron (`config.rs`).

use super::*;

/// Le minimum d'un fichier lisible : décodeur et réseau joint, un
/// vocabulaire de trois mots.
fn minimal() -> Value {
    serde_json::json!({
        "decoder": {"pred_hidden": 8, "pred_rnn_layers": 1, "vocab_size": 3},
        "joint": {"joint_hidden": 8, "activation": "relu", "encoder_hidden": 1024,
                  "pred_hidden": 8, "num_classes": 3},
        "vocabulary": ["a", "b", "c"],
        "max_symbols": 10
    })
}

#[test]
fn the_defaults_are_those_of_the_swift() {
    let config = Config::parse(&minimal().to_string()).unwrap();
    let p = &config.preprocessor;
    assert_eq!((p.sample_rate, p.features, p.n_fft), (16_000, 128, 512));
    assert_eq!((p.win_length(), p.hop_length()), (400, 160));
    assert_eq!(p.log_zero_guard_value, 2f32.powi(-24));
    let e = &config.encoder;
    assert_eq!(
        (e.n_layers, e.d_model, e.n_heads, e.conv_kernel_size),
        (24, 1024, 8, 9)
    );
    assert_eq!(e.sampling_num(), 3);
    assert!(config.prompt.is_none());
    assert_eq!(config.default_language, "en", "sans invite");
    assert_eq!(config.default_att_context_size, [56, 13]);
    assert_eq!((config.blank(), config.ms_per_frame()), (3, 80));
    assert_eq!(config.frame_seconds(), 0.08);
    assert!(config.quantization.is_none());
}

#[test]
fn the_prompt_makes_auto_the_default_language() {
    let mut json = minimal();
    json["prompt"] = serde_json::json!({"prompt_dictionary": {"fr": 8, "auto": 101}});
    let config = Config::parse(&json.to_string()).unwrap();
    let prompt = config.prompt.unwrap();
    assert_eq!((prompt.num_prompts, prompt.prompt_hidden), (128, 2048));
    assert_eq!(prompt.dictionary["fr"], 8);
    assert_eq!(config.default_language, "auto");
}

#[test]
fn nested_prednet_and_jointnet_keys_are_read() {
    let mut json = minimal();
    json["decoder"] =
        serde_json::json!({"vocab_size": 3, "prednet": {"pred_hidden": 8, "pred_rnn_layers": 2}});
    json["joint"] = serde_json::json!({"num_classes": 3, "vocabulary": ["x", "y", "z"],
        "jointnet": {"joint_hidden": 8, "activation": "relu", "encoder_hidden": 1024, "pred_hidden": 8}});
    json.as_object_mut().unwrap().remove("vocabulary");
    let config = Config::parse(&json.to_string()).unwrap();
    assert_eq!(config.decoder.prednet.pred_rnn_layers, 2);
    assert_eq!(
        config.vocabulary,
        ["x", "y", "z"],
        "vocabulaire du réseau joint"
    );
}

#[test]
fn non_standard_numbers_are_read_as_null() {
    let mut text = minimal().to_string();
    text.insert_str(1, "\"nan_value\": NaN, \"inf\": -Infinity, ");
    Config::parse(&text).unwrap();
}

#[test]
fn the_quantization_is_read_and_its_unported_forms_are_refused() {
    let with = |q: Value| {
        let mut json = minimal();
        json["quantization"] = q;
        Config::parse(&json.to_string())
    };
    let q = with(serde_json::json!({"group_size": 64, "bits": 8, "mode": "affine"}))
        .unwrap()
        .quantization;
    assert_eq!(
        q,
        Some(Quantization {
            group_size: 64,
            bits: 8
        })
    );
    for (q, needle) in [
        (
            serde_json::json!({"group_size": 32, "bits": 4, "mode": "mxfp4"}),
            "mxfp4",
        ),
        (
            serde_json::json!({"group_size": 64, "bits": 8, "joint.enc": false}),
            "joint.enc",
        ),
        (serde_json::json!({"group_size": 0, "bits": 8}), "0"),
        (serde_json::json!(8), "quantization"),
    ] {
        let err = format!("{:#}", with(q).unwrap_err());
        assert!(err.contains(needle), "{err}");
    }
}

/// Une configuration absurde, ou que le flux du Swift lirait faux, est
/// refusée par son nom ; une ligne par cas.
#[test]
fn absurd_or_unported_configurations_are_refused_by_name() {
    type Change = fn(&mut Value);
    let cases: [(Change, &str); 14] = [
        (
            |j| j["preprocessor"] = serde_json::json!({"normalize": "per_feature"}),
            "normalize",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"causal_downsampling": false}),
            "causal",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"conv_context_size": [4, 4]}),
            "convolution",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"conv_norm_type": "batch_norm"}),
            "batch_norm",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"self_attention_model": "abs_pos"}),
            "abs_pos",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"d_model": 1 << 20}),
            "d_model",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"n_heads": 0}),
            "n_heads",
        ),
        (
            |j| j["encoder"] = serde_json::json!({"n_heads": 7}),
            "incoh",
        ),
        (|j| j["decoder"]["vocab_size"] = serde_json::json!(4), "4"),
        (|j| j["max_symbols"] = serde_json::json!(0), "max_symbols"),
        (
            |j| j["default_att_context_size"] = serde_json::json!([0, 13]),
            "default_att_context_size",
        ),
        (
            |j| j["default_att_context_size"] = serde_json::json!([56, -1]),
            "default_att_context_size",
        ),
        (
            |j| j["joint"] = serde_json::json!({"activation": "relu"}),
            "num_classes",
        ),
        (|j| j["vocabulary"] = serde_json::json!([]), "0"),
    ];
    for (i, (change, needle)) in cases.into_iter().enumerate() {
        let mut json = minimal();
        change(&mut json);
        let err = format!("{:#}", Config::parse(&json.to_string()).unwrap_err());
        assert!(err.contains(needle), "cas {i} : {err}");
    }
}

#[test]
fn a_config_too_large_is_refused_without_reading_it() {
    let dir = crate::paths::test_scratch("nemotron-config-trop-grande");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(CONFIG_FILE)).unwrap();
    file.set_len(MAX_CONFIG_BYTES + 1).unwrap();
    let err = format!("{:#}", Config::load(&dir).unwrap_err());
    assert!(err.contains(&(MAX_CONFIG_BYTES + 1).to_string()), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

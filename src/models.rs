//! Catalogue, installation et suppression des modèles TTS.
//!
//! Sans modèle, le lecteur se replie sur la voix du système — utilisable, mais
//! ce n'est pas ce qu'on attend d'un lecteur. Ce module rend l'installation
//! d'un vrai modèle atteignable depuis l'interface, sans ligne de commande et
//! sans jamais télécharger quoi que ce soit sans accord explicite.

use crate::i18n::Tr;
use crate::sync::LockExt;
use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Un fichier de plus, téléchargé dans le dossier de la famille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Companion {
    pub repo: &'static str,
    pub revision: &'static str,
    /// Chemin dans le dépôt.
    pub file: &'static str,
    /// Chemin sous `models/<famille>/`. Deux paquets d'une famille qui
    /// nomment le même `dest` le partagent.
    pub dest: &'static str,
    pub bytes: u64,
}

impl Companion {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.revision, self.file)
    }
}

/// Ce qu'un paquet demande pour se charger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// Kokoro en MLX : seulement dans l'app, qui fournit le calcul.
    KokoroMlx,
    /// Kokoro par ONNX Runtime : seulement avec la feature `kokoro-onnx`.
    KokoroOnnx,
    /// Dictée, dans l'app seulement : calcul MLX côté Swift.
    ParakeetMlx,
    /// Dictée, dans l'app seulement : Parakeet en MLX côté Swift, poids de
    /// Phonon (archive `.tar.zst` que l'app lit telle quelle).
    PhononMlx,
    /// Dictée, dans l'app seulement : calcul MLX côté Swift.
    WhisperMlx,
    /// Dictée, dans l'app seulement : calcul MLX côté Swift.
    CanaryMlx,
    /// Dictée, dans l'app seulement : calcul MLX côté Swift.
    VoxtralRealtimeMlx,
    /// Dictée, dans l'app seulement : Nemotron ASR (RNN-T en flux) en MLX
    /// côté Swift.
    NemotronMlx,
    /// Dictée, dans l'app seulement : Qwen3-ASR en MLX côté Swift
    /// (vocabulaire BPE à côté des poids).
    Qwen3AsrMlx,
    /// Dictée, dans l'app seulement : Cohere Transcribe en MLX côté Swift
    /// (vocabulaire SentencePiece à côté des poids).
    CohereMlx,
    /// Lecture à partir du texte, dans l'app seulement : Qwen3-TTS en MLX
    /// (calcul Swift, `TextSpeechCompute`).
    Qwen3TtsMlx,
    /// Lecture à partir du texte, dans l'app seulement : MagpieTTS en MLX.
    MagpieMlx,
}

/// Ce que fait un modèle : lire à voix haute, ou écrire ce qu'on dicte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Speech,
    Dictation,
}

/// Un paquet téléchargeable : une variante de précision d'une famille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPackage {
    pub id: &'static str,
    pub family: &'static str,
    pub display_name: Tr,
    pub description: Tr,
    /// Précision des poids, qui arbitre taille contre qualité.
    pub precision: Tr,
    pub languages: &'static [&'static str],
    pub repo: &'static str,
    pub revision: &'static str,
    /// Chemin du fichier dans le dépôt.
    pub file: &'static str,
    /// Comment annoncer la couverture linguistique.
    ///
    /// Certains modèles annoncent un ordre de grandeur plutôt qu'une liste ;
    /// afficher « 31 langues » pour un modèle qui en revendique six cents
    /// serait faux dans l'autre sens.
    pub languages_label: Tr,
    /// Taille annoncée, relevée sur le dépôt.
    pub bytes: u64,
    /// Recommandé comme premier modèle.
    pub recommended: bool,
    /// Ce que la lecture donnera : fluide, ou hachée parce que le modèle
    /// produit moins vite qu'il ne parle. Mesuré, pas promis.
    pub pace: Tr,
    /// Fichiers téléchargés à côté du fichier principal.
    pub companions: &'static [Companion],
    pub runtime: Runtime,
    pub kind: Kind,
    /// Les voix, pour la fiche (« Française : … · Anglaise : … »).
    pub voices_label: Tr,
    /// Les licences, pour la fiche.
    pub licenses: Tr,
}

impl ModelPackage {
    /// Taille annoncée du paquet entier, compagnons compris.
    pub fn total_bytes(&self) -> u64 {
        self.bytes + self.companions.iter().map(|c| c.bytes).sum::<u64>()
    }

    /// Nom du fichier une fois installé.
    pub fn file_name(&self) -> &str {
        self.file.rsplit('/').next().unwrap_or(self.file)
    }

    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    pub fn speaks(&self, language: &str) -> bool {
        let code = language.split(['-', '_']).next().unwrap_or(language);
        self.languages.contains(&code)
    }
}

const KOKORO_MLX_REPO: &str = "mlx-community/Kokoro-82M-bf16";
const KOKORO_MLX_REVISION: &str = "a71e4d38b236d968966a2002c4c895dbd12b1c3c";
const KOKORO_ONNX_REPO: &str = "onnx-community/Kokoro-82M-v1.0-ONNX-timestamped";
const KOKORO_ONNX_REVISION: &str = "dd4401a9add81ac692d20e240d22ec9dda82cc29";
const KOKORO_LANGUAGES: &[&str] = &["fr", "en"];
/// Une voix Kokoro : 510 × 256 flottants, en safetensors.
pub const KOKORO_VOICE_BYTES: u64 = 522_320;

macro_rules! kokoro_voice {
    ($name:literal) => {
        Companion {
            repo: KOKORO_MLX_REPO,
            revision: KOKORO_MLX_REVISION,
            file: concat!("voices/", $name, ".safetensors"),
            dest: concat!("voices/", $name, ".safetensors"),
            bytes: KOKORO_VOICE_BYTES,
        }
    };
}

/// Ce que les deux paquets Kokoro partagent : la configuration (elle porte
/// le vocabulaire des phonèmes) et les voix. `p-fenrir60` n'y est pas : elle
/// est livrée avec Dysflow.
///
/// Les 29 voix du paquet sont toutes proposées (`KOKORO_VOICES`) ; `ff_siwis`
/// et `am_fenrir` refont aussi `p-fenrir60` (`examples/kokoro_blend.rs`).
const KOKORO_SHARED: &[Companion] = &[
    Companion {
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "config.json",
        dest: "config.json",
        bytes: 2_351,
    },
    kokoro_voice!("ff_siwis"),
    kokoro_voice!("af_bella"),
    kokoro_voice!("af_alloy"),
    kokoro_voice!("af_aoede"),
    kokoro_voice!("af_heart"),
    kokoro_voice!("af_jessica"),
    kokoro_voice!("af_kore"),
    kokoro_voice!("af_nicole"),
    kokoro_voice!("af_nova"),
    kokoro_voice!("af_river"),
    kokoro_voice!("af_sarah"),
    kokoro_voice!("af_sky"),
    kokoro_voice!("am_adam"),
    kokoro_voice!("am_echo"),
    kokoro_voice!("am_eric"),
    kokoro_voice!("am_fenrir"),
    kokoro_voice!("am_liam"),
    kokoro_voice!("am_michael"),
    kokoro_voice!("am_onyx"),
    kokoro_voice!("am_puck"),
    kokoro_voice!("am_santa"),
    kokoro_voice!("bf_alice"),
    kokoro_voice!("bf_emma"),
    kokoro_voice!("bf_isabella"),
    kokoro_voice!("bf_lily"),
    kokoro_voice!("bm_daniel"),
    kokoro_voice!("bm_fable"),
    kokoro_voice!("bm_george"),
    kokoro_voice!("bm_lewis"),
];

const KOKORO_VOICES_LABEL: Tr = Tr::new(
    "30 voices: p-fenrir60 (French, recommended), af_bella (American, recommended)…",
    "30 voix : p-fenrir60 (française, recommandée), af_bella (américaine, recommandée)…",
);

/// Un compagnon rangé sous le même chemin que dans le dépôt.
const fn hf(repo: &'static str, revision: &'static str, file: &'static str, bytes: u64) -> Companion {
    Companion { repo, revision, file, dest: file, bytes }
}

const QWEN3_06B_REPO: &str = "mlx-community/Qwen3-TTS-12Hz-0.6B-Base-8bit";
const QWEN3_06B_REVISION: &str = "50f45ef0047cde7e84c2ef04326acb8ada2436a7";
const MAGPIE_REPO: &str = "aufklarer/Magpie-TTS-Multilingual-357M-MLX-8bit";
const MAGPIE_REVISION: &str = "260839c0a2c46993735d72082c3fba177710d477";

/// Ce que lit le chargeur de Qwen3 (relevé §1.e) à côté de
/// `model.safetensors` : configuration, vocabulaire BPE, codec. Ni
/// `model.safetensors.index.json`, ni `generation_config.json`, ni
/// `preprocessor_config.json`, que personne ne lit.
const QWEN3_06B_FILES: &[Companion] = &[
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "config.json", 5_522),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "vocab.json", 2_776_833),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "merges.txt", 1_671_839),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "tokenizer_config.json", 7_344),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "speech_tokenizer/config.json", 2_336),
    hf(QWEN3_06B_REPO, QWEN3_06B_REVISION, "speech_tokenizer/model.safetensors", 682_293_092),
];

/// Magpie (relevé §3.d) : encodeur de texte, décodeur (`decoder_prefill`
/// seulement : `decoder_step` porte les mêmes poids et n'est pas lu), codec,
/// et les vocabulaires français et anglais.
const MAGPIE_FILES: &[Companion] = &[
    hf(MAGPIE_REPO, MAGPIE_REVISION, "text_encoder/config.json", 2_461),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "text_encoder/model.safetensors", 108_912_472),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "decoder_prefill/config.json", 9_650),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "decoder_prefill/model.safetensors", 129_439_762),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "nanocodec_decoder/config.json", 466),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "nanocodec_decoder/model.safetensors", 63_165_546),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "tokenizer/fr.json", 29_555),
    hf(MAGPIE_REPO, MAGPIE_REVISION, "tokenizer/en.json", 29_699),
];

const PARAKEET_V3_REPO: &str = "animaslabs/parakeet-tdt-0.6b-v3-mlx-8bit";
const PARAKEET_V3_REVISION: &str = "18498133db8b4c8753bf98b3c5b6639b2a791be3";
const PARAKEET_V2_REPO: &str = "mlx-community/parakeet-tdt-0.6b-v2";
const PARAKEET_V2_REVISION: &str = "8ae155301e23d820d82aa60d24817c900e69e487";
const NEMOTRON_REPO: &str = "mlx-community/nemotron-3.5-asr-streaming-0.6b-8bit";
const NEMOTRON_REVISION: &str = "7279359e4481b5e9e185a318bd618e429c6d86cd";
const QWEN3_ASR_06B_REPO: &str = "mlx-community/Qwen3-ASR-0.6B-8bit";
const QWEN3_ASR_06B_REVISION: &str = "89e96d92ba34aca20b3e29fb10cc284097d1219f";
const QWEN3_ASR_17B_REPO: &str = "mlx-community/Qwen3-ASR-1.7B-4bit";
const QWEN3_ASR_17B_REVISION: &str = "78a389c776a5483b2d0d4ea5494e11012e0d6159";
/// Cohere Transcribe : la conversion de beshkenadze, celle sur laquelle le
/// port Swift de mlx-audio-swift a été validé. Le miroir
/// `mlx-community/cohere-transcribe-03-2026-mlx-8bit` n'est pas quantifié
/// (4,13 Go de BF16 malgré son nom et sa config) et suit une autre
/// disposition des poids (celle de mlx-speech) que ce port ne lit pas.
const COHERE_REPO: &str = "beshkenadze/cohere-transcribe-03-2026-mlx-8bit";
const COHERE_REVISION: &str = "d1f843476f84846e6fe7aa58a6033f17882f0ec9";

/// Ce que lit le chargeur de Qwen3-ASR à côté de `model.safetensors` :
/// configuration et vocabulaire BPE (le même que Qwen3-TTS, lu par
/// `Qwen2BPETokenizer`). Ni `model.safetensors.index.json`, ni
/// `generation_config.json`, `preprocessor_config.json`,
/// `chat_template.json`, que personne ne lit.
const QWEN3_ASR_06B_FILES: &[Companion] = &[
    hf(QWEN3_ASR_06B_REPO, QWEN3_ASR_06B_REVISION, "config.json", 7_187),
    hf(QWEN3_ASR_06B_REPO, QWEN3_ASR_06B_REVISION, "vocab.json", 2_776_833),
    hf(QWEN3_ASR_06B_REPO, QWEN3_ASR_06B_REVISION, "merges.txt", 1_671_853),
    hf(QWEN3_ASR_06B_REPO, QWEN3_ASR_06B_REVISION, "tokenizer_config.json", 12_487),
];
const QWEN3_ASR_17B_FILES: &[Companion] = &[
    hf(QWEN3_ASR_17B_REPO, QWEN3_ASR_17B_REVISION, "config.json", 7_188),
    hf(QWEN3_ASR_17B_REPO, QWEN3_ASR_17B_REVISION, "vocab.json", 2_776_833),
    hf(QWEN3_ASR_17B_REPO, QWEN3_ASR_17B_REVISION, "merges.txt", 1_671_853),
    hf(QWEN3_ASR_17B_REPO, QWEN3_ASR_17B_REVISION, "tokenizer_config.json", 12_487),
];
/// Cohere : configuration, vocabulaire SentencePiece et jetons spéciaux
/// (`added_tokens_decoder`). Ni `key_map.json`, ni
/// `conversion_summary.json`, `preprocessor_config.json`,
/// `special_tokens_map.json`.
const COHERE_FILES: &[Companion] = &[
    hf(COHERE_REPO, COHERE_REVISION, "config.json", 4_336),
    hf(COHERE_REPO, COHERE_REVISION, "tokenizer.model", 492_827),
    hf(COHERE_REPO, COHERE_REVISION, "tokenizer_config.json", 48_141),
];

/// Modèles proposés à l'installation : Kokoro, en MLX (l'app) ou en int8
/// par ONNX Runtime (le terminal), et dans l'app Qwen3-TTS et MagpieTTS
/// (lot 3). Sans eux, la voix du système lit.
///
/// Supertonic et MagpieTTS n'y sont plus (14 oct. 2026), et leur moteur,
/// audio.cpp, est sorti du projet le même jour : un seul modèle, c'est un
/// choix de moins à faire.
pub const CATALOG: &[ModelPackage] = &[
    ModelPackage {
        id: "kokoro_82m_mlx",
        family: "kokoro",
        display_name: Tr::new("Kokoro 82M · MLX", "Kokoro 82M · MLX"),
        description: Tr::new("Natural, light voice, computed by the GPU.", "Voix naturelle et légère, calculée par la puce graphique."),
        precision: Tr::new("bf16", "bf16"),
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_MLX_REPO,
        revision: KOKORO_MLX_REVISION,
        file: "kokoro-v1_0.safetensors",
        languages_label: Tr::new("French, English", "Français, anglais"),
        bytes: 327_115_152,
        recommended: true,
        pace: Tr::new("Fifteen times faster than speech on M5: smooth reading.", "Quinze fois plus vite que la parole sur M5 : lecture fluide."),
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroMlx,
        kind: Kind::Speech,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: Tr::new("Weights and voices Apache-2.0 · MLX code MIT · espeak-ng GPL-3.0 (separate program)", "Poids et voix Apache-2.0 · code MLX MIT · espeak-ng GPL-3.0 (programme séparé)"),
    },
    ModelPackage {
        id: "kokoro_82m_int8",
        family: "kokoro",
        display_name: Tr::new("Kokoro 82M · int8", "Kokoro 82M · int8"),
        description: Tr::new("Natural, light voice, computed by the CPU (ONNX Runtime ≥ 1.31).", "Voix naturelle et légère, calculée par le processeur (ONNX Runtime ≥ 1.31)."),
        precision: Tr::new("int8", "int8"),
        languages: KOKORO_LANGUAGES,
        repo: KOKORO_ONNX_REPO,
        revision: KOKORO_ONNX_REVISION,
        file: "onnx/model_quantized.onnx",
        languages_label: Tr::new("French, English", "Français, anglais"),
        bytes: 92_361_055,
        recommended: true,
        pace: Tr::new("Eight to ten times faster than speech on CPU: smooth reading.", "Huit à dix fois plus vite que la parole sur processeur : lecture fluide."),
        companions: KOKORO_SHARED,
        runtime: Runtime::KokoroOnnx,
        kind: Kind::Speech,
        voices_label: KOKORO_VOICES_LABEL,
        licenses: Tr::new("Weights and voices Apache-2.0 · ONNX Runtime MIT · espeak-ng GPL-3.0 (separate program)", "Poids et voix Apache-2.0 · ONNX Runtime MIT · espeak-ng GPL-3.0 (programme séparé)"),
    },
    ModelPackage {
        id: "parakeet_tdt_v3",
        family: "parakeet_tdt_v3",
        display_name: Tr::new("Parakeet v3", "Parakeet v3"),
        description: Tr::new("Fast, accurate recognition of twenty-five European languages, on the GPU.", "Reconnaît vite et bien vingt-cinq langues européennes, sur la puce graphique."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        // Quantifié en 8 bits (908 Mo) : le paquet de mlx-community, en F32,
        // pesait 2,5 Go pour le même modèle. Même `config.json`, plus sa clé
        // `quantization`, que le chargeur lit.
        repo: PARAKEET_V3_REPO,
        revision: PARAKEET_V3_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("25 European languages", "25 langues européennes"),
        bytes: 908_392_684,
        recommended: true,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[
            Companion {
                repo: PARAKEET_V3_REPO,
                revision: PARAKEET_V3_REVISION,
                file: "config.json",
                dest: "config.json",
                bytes: 318_494,
            },
        ],
        runtime: Runtime::ParakeetMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("CC-BY-4.0 (weights and code)", "CC-BY-4.0 (poids et code)"),
    },
    ModelPackage {
        id: "phonon_2",
        family: "phonon_2",
        display_name: Tr::new("Phonon-2", "Phonon-2"),
        description: Tr::new("Parakeet v3 retrained for English and shrunk to 164 MB, almost as accurate. French works, less reliably.", "Parakeet v3 réentraîné pour l'anglais et réduit à 164 Mo, presque aussi juste. Le français passe, moins sûrement."),
        precision: Tr::new("≈ 2-bit", "≈ 2 bits"),
        languages: &["en", "fr"],
        repo: "FermionResearch/Phonon-2",
        revision: "7e153e4054c0a10db6d47d6fff5c062d4c77c154",
        // Poids et `config.json` dans une seule archive, lue par l'app.
        file: "phonon-2.bps.tar.zst",
        languages_label: Tr::new("English (French less reliable)", "Anglais (français moins sûr)"),
        bytes: 163_515_201,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[],
        runtime: Runtime::PhononMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("CC-BY-4.0 weights (Fermion Research, based on NVIDIA)", "Poids CC-BY-4.0 (Fermion Research, d'après NVIDIA)"),
    },
    ModelPackage {
        id: "whisper_tiny_4bit",
        family: "whisper_tiny_4bit",
        display_name: Tr::new("Whisper tiny", "Whisper tiny"),
        description: Tr::new("The smallest Whisper: fast, for a short text or a modest machine.", "Le plus petit Whisper : rapide, pour un texte court ou une machine juste."),
        precision: Tr::new("4-bit", "4 bits"),
        languages: &["fr", "en"],
        repo: "mlx-community/whisper-tiny-4bit",
        revision: "064afea920b0373f8e19d51dc698bfbb91f501ff",
        file: "model.safetensors",
        languages_label: Tr::new("99 languages", "99 langues"),
        bytes: 22_029_431,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[
            Companion {
                repo: "mlx-community/whisper-tiny-4bit",
                revision: "064afea920b0373f8e19d51dc698bfbb91f501ff",
                file: "config.json",
                dest: "config.json",
                bytes: 335,
            },
            Companion {
                repo: "mlx-community/whisper-tiny-4bit",
                revision: "064afea920b0373f8e19d51dc698bfbb91f501ff",
                file: "multilingual.tiktoken",
                dest: "multilingual.tiktoken",
                bytes: 816_730,
            },
        ],
        runtime: Runtime::WhisperMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "whisper_small_4bit",
        family: "whisper_small_4bit",
        display_name: Tr::new("Whisper small", "Whisper small"),
        description: Tr::new("A good trade-off between size and accuracy, in ninety-nine languages.", "Un bon compromis entre taille et justesse, en quatre-vingt-dix-neuf langues."),
        precision: Tr::new("4-bit", "4 bits"),
        languages: &["fr", "en"],
        repo: "mlx-community/whisper-small-4bit",
        revision: "1ac90abbb1b2f53863bfd2a483a3c31d792c3e5b",
        file: "model.safetensors",
        languages_label: Tr::new("99 languages", "99 langues"),
        bytes: 139_111_057,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[
            Companion {
                repo: "mlx-community/whisper-small-4bit",
                revision: "1ac90abbb1b2f53863bfd2a483a3c31d792c3e5b",
                file: "config.json",
                dest: "config.json",
                bytes: 339,
            },
            Companion {
                repo: "mlx-community/whisper-small-4bit",
                revision: "1ac90abbb1b2f53863bfd2a483a3c31d792c3e5b",
                file: "multilingual.tiktoken",
                dest: "multilingual.tiktoken",
                bytes: 816_730,
            },
        ],
        runtime: Runtime::WhisperMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "whisper_large_v3_turbo_4bit",
        family: "whisper_large_v3_turbo_4bit",
        display_name: Tr::new("Whisper large-v3-turbo", "Whisper large-v3-turbo"),
        description: Tr::new("The most accurate Whisper in the list, at the cost of a heavier download.", "Le plus juste des Whisper de la liste, au prix d'un téléchargement plus lourd."),
        precision: Tr::new("4-bit", "4 bits"),
        languages: &["fr", "en"],
        repo: "mlx-community/whisper-large-v3-turbo-4bit",
        revision: "0f058d38170d183f9fdee07908f5b515d91793a8",
        file: "model.safetensors",
        languages_label: Tr::new("99 languages", "99 langues"),
        bytes: 463_462_982,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[
            Companion {
                repo: "mlx-community/whisper-large-v3-turbo-4bit",
                revision: "0f058d38170d183f9fdee07908f5b515d91793a8",
                file: "config.json",
                dest: "config.json",
                bytes: 341,
            },
            Companion {
                repo: "mlx-community/whisper-large-v3-turbo-4bit",
                revision: "0f058d38170d183f9fdee07908f5b515d91793a8",
                file: "multilingual.tiktoken",
                dest: "multilingual.tiktoken",
                bytes: 816_730,
            },
        ],
        runtime: Runtime::WhisperMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "canary_1b_v2_q8",
        family: "canary_1b_v2_q8",
        display_name: Tr::new("Canary 1b-v2", "Canary 1b-v2"),
        description: Tr::new("Recognition and translation of twenty-five European languages, quantized to 8 bits.", "Reconnaissance et traduction de vingt-cinq langues européennes, quantifié en 8 bits."),
        precision: Tr::new("q8", "q8"),
        languages: &["fr", "en"],
        repo: "Mediform/canary-1b-v2-mlx-q8",
        revision: "0b6b32ee10f30c89e3ead7249bb636445e3019ee",
        file: "model.safetensors",
        languages_label: Tr::new("25 European languages", "25 langues européennes"),
        bytes: 1_136_436_574,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        companions: &[
            Companion {
                repo: "Mediform/canary-1b-v2-mlx-q8",
                revision: "0b6b32ee10f30c89e3ead7249bb636445e3019ee",
                file: "config.json",
                dest: "config.json",
                bytes: 673_039,
            },
        ],
        runtime: Runtime::CanaryMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("CC-BY-4.0", "CC-BY-4.0"),
    },
    ModelPackage {
        id: "voxtral_realtime_4bit",
        family: "voxtral_realtime_4bit",
        display_name: Tr::new("Voxtral Realtime", "Voxtral Realtime"),
        description: Tr::new("Writes as you speak, without waiting for the end of the sentence.", "Écrit au fil de la parole, sans attendre la fin de la phrase."),
        precision: Tr::new("4-bit", "4 bits"),
        languages: &["fr", "en"],
        repo: "mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit",
        revision: "fdebf7b2af834a1db4b8a3c99ab7480b333adf9e",
        file: "model.safetensors",
        languages_label: Tr::new("13 languages", "13 langues"),
        bytes: 3_133_798_126,
        recommended: false,
        pace: Tr::new("As you speak", "Au fil de la parole"),
        companions: &[
            Companion {
                repo: "mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit",
                revision: "fdebf7b2af834a1db4b8a3c99ab7480b333adf9e",
                file: "config.json",
                dest: "config.json",
                bytes: 1_513,
            },
            Companion {
                repo: "mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit",
                revision: "fdebf7b2af834a1db4b8a3c99ab7480b333adf9e",
                file: "tekken.json",
                dest: "tekken.json",
                bytes: 14_910_348,
            },
        ],
        runtime: Runtime::VoxtralRealtimeMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "parakeet_tdt_v2",
        family: "parakeet_tdt_v2",
        display_name: Tr::new("Parakeet v2", "Parakeet v2"),
        description: Tr::new("The English Parakeet: slightly more accurate than v3 in English, but it only knows English.", "Le Parakeet anglais : un peu plus juste que v3 en anglais, mais il ne connaît que l'anglais."),
        precision: Tr::new("bf16", "bf16"),
        languages: &["en"],
        repo: PARAKEET_V2_REPO,
        revision: PARAKEET_V2_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("English", "Anglais"),
        bytes: 2_471_559_904,
        recommended: false,
        pace: Tr::new("Re-read every 1.5 s", "Relecture toutes les 1,5 s"),
        // Le vocabulaire est dans `config.json` (`labels`), comme pour v3.
        companions: &[hf(PARAKEET_V2_REPO, PARAKEET_V2_REVISION, "config.json", 36_176)],
        runtime: Runtime::ParakeetMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("CC-BY-4.0 (weights and code)", "CC-BY-4.0 (poids et code)"),
    },
    ModelPackage {
        id: "nemotron_35_asr_streaming_8bit",
        family: "nemotron_35_asr_streaming_8bit",
        display_name: Tr::new("Nemotron 3.5 Streaming", "Nemotron 3.5 Streaming"),
        description: Tr::new("Writes as you speak, each bit of audio read only once. Less accurate than Parakeet v3 in French (about 9% word error rate, versus 5%).", "Écrit au fil de la parole, chaque bout d'audio lu une seule fois. Moins juste que Parakeet v3 en français (environ 9 % de mots faux, contre 5 %)."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        repo: NEMOTRON_REPO,
        revision: NEMOTRON_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("40 languages", "40 langues"),
        bytes: 755_598_923,
        recommended: false,
        pace: Tr::new("As you speak: 1.5 s of compute for 36 s of speech (M5)", "Au fil de la parole : 1,5 s de calcul pour 36 s de parole (M5)"),
        // Vocabulaire et invites de langue dans `config.json` ; ni
        // `tokenizer.model` ni `vocab.txt`, que le chargeur ne lit pas.
        companions: &[hf(NEMOTRON_REPO, NEMOTRON_REVISION, "config.json", 159_605)],
        runtime: Runtime::NemotronMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("OpenMDW-1.1 weights (NVIDIA)", "Poids OpenMDW-1.1 (NVIDIA)"),
    },
    ModelPackage {
        id: "qwen3_asr_06b_8bit",
        family: "qwen3_asr_06b_8bit",
        display_name: Tr::new("Qwen3-ASR 0.6B", "Qwen3-ASR 0.6B"),
        description: Tr::new("Writes in the dictation language, among thirty. Writes the text token by token: each re-read takes longer than with Parakeet.", "Écrit dans la langue de la dictée, parmi trente. Écrit le texte jeton par jeton : chaque relecture prend plus de temps qu'avec Parakeet."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        repo: QWEN3_ASR_06B_REPO,
        revision: QWEN3_ASR_06B_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("30 languages", "30 langues"),
        bytes: 1_006_229_426,
        recommended: false,
        pace: Tr::new("Sliding re-read: 0.4 s of compute for 9 s of speech, 1.5 s for 36 s (M5)", "Relecture glissante : 0,4 s de calcul pour 9 s de parole, 1,5 s pour 36 s (M5)"),
        companions: QWEN3_ASR_06B_FILES,
        runtime: Runtime::Qwen3AsrMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "qwen3_asr_17b_4bit",
        family: "qwen3_asr_17b_4bit",
        display_name: Tr::new("Qwen3-ASR 1.7B", "Qwen3-ASR 1.7B"),
        description: Tr::new("The large Qwen3-ASR, more accurate than the 0.6B, shrunk to 4 bits. Writes in the dictation language; the slower of the two to re-read.", "Le grand Qwen3-ASR, plus juste que le 0.6B, réduit à 4 bits. Écrit dans la langue de la dictée ; le plus lent des deux à relire."),
        precision: Tr::new("4-bit", "4 bits"),
        languages: &["fr", "en"],
        repo: QWEN3_ASR_17B_REPO,
        revision: QWEN3_ASR_17B_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("30 languages", "30 langues"),
        bytes: 1_603_081_617,
        recommended: false,
        pace: Tr::new("Sliding re-read: 0.6 s of compute for 9 s of speech, 2.4 s for 36 s (M5)", "Relecture glissante : 0,6 s de calcul pour 9 s de parole, 2,4 s pour 36 s (M5)"),
        companions: QWEN3_ASR_17B_FILES,
        runtime: Runtime::Qwen3AsrMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0", "Apache-2.0"),
    },
    ModelPackage {
        id: "cohere_transcribe_8bit",
        family: "cohere_transcribe_8bit",
        display_name: Tr::new("Cohere Transcribe", "Cohere Transcribe"),
        description: Tr::new("Top of the Open ASR leaderboard in English (5.4% word error rate on average). It does not guess the language: it uses the one in the settings.", "Le plus juste du classement Open ASR en anglais (5,4 % de mots faux en moyenne). Il ne devine pas la langue : il prend celle du réglage."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        repo: COHERE_REPO,
        revision: COHERE_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("14 languages, chosen in the settings", "14 langues, choisie dans le réglage"),
        bytes: 2_418_031_831,
        recommended: false,
        pace: Tr::new("Sliding re-read: 0.4 s of compute for 9 s of speech, 1.8 s for 36 s (M5)", "Relecture glissante : 0,4 s de calcul pour 9 s de parole, 1,8 s pour 36 s (M5)"),
        companions: COHERE_FILES,
        runtime: Runtime::CohereMlx,
        kind: Kind::Dictation,
        voices_label: Tr::new("", ""),
        licenses: Tr::new("Apache-2.0 weights (Cohere)", "Poids Apache-2.0 (Cohere)"),
    },
    ModelPackage {
        id: "qwen3_tts_06b_base_8bit",
        family: "qwen3_tts_06b_base_8bit",
        display_name: Tr::new("Qwen3 0.6B · French voice", "Qwen3 0.6B · voix française"),
        description: Tr::new("A fixed French voice, with a timbre drawn from SIWIS, computed by the GPU. Approximate word tracking. With the other Qwen installed, each keeps its own copy of the sound decoder (682 MB).", "Une voix française fixe, au timbre tiré de SIWIS, calculée par la puce graphique. Suivi des mots approché. Avec l'autre Qwen installé, chacun garde sa copie du décodeur de son (682 Mo)."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        repo: QWEN3_06B_REPO,
        revision: QWEN3_06B_REVISION,
        file: "model.safetensors",
        languages_label: Tr::new("10 languages, including French and English", "10 langues, dont français et anglais"),
        bytes: 1_304_461_214,
        recommended: false,
        pace: Tr::new("Heavy: choppy reading above 1× (busy 76% of the time at 1× on M5).", "Lourd : lecture hachée au-delà de 1× (occupé 76 % du temps à 1× sur M5)."),
        companions: QWEN3_06B_FILES,
        runtime: Runtime::Qwen3TtsMlx,
        kind: Kind::Speech,
        voices_label: Tr::new("1 voice: siwis (French, fixed)", "1 voix : siwis (française, fixe)"),
        licenses: Tr::new("Apache-2.0 weights · MLX code MIT (mlx-audio-swift) · timbre drawn from SIWIS (CC-BY-4.0, Idiap)", "Poids Apache-2.0 · code MLX MIT (mlx-audio-swift) · timbre tiré de SIWIS (CC-BY-4.0, Idiap)"),
    },
    // Qwen3 1.7B CustomVoice retiré le 17 oct. 2026 : mesuré à 110 % du temps occupé à 1× sur M5
    // (lecture hachée même à vitesse normale) ; à reprendre après optimisation du port Swift.
    ModelPackage {
        id: "magpie_tts_357m_8bit",
        family: "magpie_tts_357m_8bit",
        display_name: Tr::new("MagpieTTS 357M", "MagpieTTS 357M"),
        description: Tr::new("NVIDIA's multilingual model, five English voices, French read as is. Approximate word tracking.", "Modèle multilingue de NVIDIA, cinq voix anglaises, le français lu tel quel. Suivi des mots approché."),
        precision: Tr::new("8-bit", "8 bits"),
        languages: &["fr", "en"],
        repo: MAGPIE_REPO,
        revision: MAGPIE_REVISION,
        file: "config.json",
        languages_label: Tr::new("9 languages, including French and English", "9 langues, dont français et anglais"),
        bytes: 32,
        recommended: false,
        pace: Tr::new("Heavy: choppy reading above 1× (busy 69% of the time at 1× on M5).", "Lourd : lecture hachée au-delà de 1× (occupé 69 % du temps à 1× sur M5)."),
        companions: MAGPIE_FILES,
        runtime: Runtime::MagpieMlx,
        kind: Kind::Speech,
        voices_label: Tr::new("5 English voices: sofia (recommended), aria, jason, leo, john_van_stan", "5 voix anglaises : sofia (recommandée), aria, jason, leo, john_van_stan"),
        licenses: Tr::new("NVIDIA Open Model License weights (downloaded by you) · Apache-2.0 code (speech-swift) · CMUdict English dictionary (BSD)", "Poids NVIDIA Open Model License (téléchargés par vous) · code Apache-2.0 (speech-swift) · dictionnaire anglais CMUdict (BSD)"),
    },
];

/// Catalogue restreint à ce que ce binaire sait charger.
///
/// Kokoro MLX : seulement quand l'app fournit le calcul (`mlx`), et alors
/// jamais le paquet ONNX à côté. Kokoro int8 : seulement avec la feature
/// `kokoro-onnx`.
pub fn available_with(mlx: bool) -> Vec<&'static ModelPackage> {
    CATALOG
        .iter()
        .filter(|p| match p.runtime {
            Runtime::KokoroMlx
            | Runtime::ParakeetMlx
            | Runtime::PhononMlx
            | Runtime::WhisperMlx
            | Runtime::CanaryMlx
            | Runtime::VoxtralRealtimeMlx
            | Runtime::NemotronMlx
            | Runtime::Qwen3AsrMlx
            | Runtime::CohereMlx
            | Runtime::Qwen3TtsMlx
            | Runtime::MagpieMlx => mlx,
            Runtime::KokoroOnnx => !mlx && cfg!(feature = "kokoro-onnx"),
        })
        .collect()
}

/// Le catalogue du terminal : sans calcul MLX.
pub fn available() -> Vec<&'static ModelPackage> {
    available_with(false)
}

/// Répertoire où sont rangés les modèles installés.
pub fn models_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|d| d.join("models"))
}

pub fn install_path(package: &ModelPackage) -> Option<PathBuf> {
    models_dir().map(|d| main_path_in(&d, package))
}

fn main_path_in(root: &Path, package: &ModelPackage) -> PathBuf {
    root.join(package.family).join(package.file_name())
}

fn companion_path_in(root: &Path, package: &ModelPackage, companion: &Companion) -> PathBuf {
    root.join(package.family).join(companion.dest)
}

/// Ce qu'il faut télécharger pour ce paquet : adresse, destination, taille.
pub(crate) fn planned_files_in(root: &Path, package: &ModelPackage) -> Vec<(String, PathBuf, u64)> {
    let mut files = vec![(package.url(), main_path_in(root, package), package.bytes)];
    files.extend(
        package
            .companions
            .iter()
            .map(|c| (c.url(), companion_path_in(root, package, c), c.bytes)),
    );
    files
}

/// Vrai si le `model_path` configuré désigne ce fichier installé.
///
/// Le moteur développe le `~` et suit les liens : une configuration écrite à
/// la main (`~/…`, ou via un dossier lié) désigne le même fichier qu'un chemin
/// absolu. Une simple égalité de textes laisserait supprimer le modèle en
/// cours d'usage.
pub fn is_active_path(configured: &str, installed: &Path) -> bool {
    is_active_path_with(configured, installed, dirs::home_dir().as_deref())
}

fn is_active_path_with(configured: &str, installed: &Path, home: Option<&Path>) -> bool {
    if configured.is_empty() {
        return false;
    }
    let expanded = PathBuf::from(crate::paths::expand_tilde_with(configured, home));
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canonical(&expanded) == canonical(installed)
}

/// Vrai si `configured` désigne le fichier installé de ce modèle.
pub fn is_active_package(configured: &str, package: &ModelPackage) -> bool {
    install_path(package).is_some_and(|p| is_active_path(configured, &p))
}

/// Taille sur disque du fichier principal, s'il est là.
///
/// Un téléchargement interrompu laisse un `.part`, qui n'est jamais confondu
/// avec un modèle complet.
pub fn installed_size(package: &ModelPackage) -> Option<u64> {
    let path = install_path(package)?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn size_of(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// Installé : chaque fichier, le principal comme ses compagnons, a sa taille
/// attendue. Un principal tronqué n'est pas installé : la migration ne doit
/// pas basculer dessus.
pub fn is_installed_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)) == Some(package.bytes)
        && package
            .companions
            .iter()
            .all(|c| size_of(&companion_path_in(root, package, c)) == Some(c.bytes))
}

pub fn is_installed(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_installed_in(&root, package))
}

/// Vrai si un fichier du paquet est là sans avoir la taille annoncée.
///
/// Un téléchargement interrompu ailleurs que par nous, un disque plein, une
/// copie tronquée : le fichier existe et se présente comme installé, puis le
/// moteur refuse de l'ouvrir sur un message illisible. Mieux vaut le dire.
pub fn is_truncated_in(root: &Path, package: &ModelPackage) -> bool {
    size_of(&main_path_in(root, package)).is_some_and(|s| s != package.bytes)
        || package.companions.iter().any(|c| {
            size_of(&companion_path_in(root, package, c)).is_some_and(|s| s != c.bytes)
        })
}

pub fn is_truncated(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| is_truncated_in(&root, package))
}

/// Premier modèle installé, celui que le lecteur choisit par défaut.
pub fn first_installed() -> Option<&'static ModelPackage> {
    first_installed_with(false)
}

/// Comme `first_installed`, pour un lecteur dont l'app fournit (ou non) le
/// calcul MLX.
pub fn first_installed_with(mlx: bool) -> Option<&'static ModelPackage> {
    available_with(mlx).into_iter().find(|p| p.kind == Kind::Speech && is_installed(p))
}

/// Le paquet que l'installation automatique télécharge : le Kokoro
/// recommandé de ce catalogue, quand aucun Kokoro du catalogue n'est
/// installé. `None` : déjà là, ou pas de Kokoro dans ce binaire.
///
/// Seul compte ce que ce binaire sait charger : un Kokoro int8 posé par le
/// terminal ne dispense pas l'app (MLX) d'installer le sien.
pub fn default_voice_to_install(
    available: &[&'static ModelPackage],
    installed: impl Fn(&ModelPackage) -> bool,
) -> Option<&'static ModelPackage> {
    let kokoro: Vec<&'static ModelPackage> = available.iter().copied().filter(|p| p.family == "kokoro").collect();
    if kokoro.iter().any(|p| installed(p)) {
        return None;
    }
    kokoro.iter().copied().find(|p| p.recommended).or_else(|| kokoro.first().copied())
}

/// L'installation automatique de la voix, sans rien demander : démarre le
/// téléchargement du Kokoro recommandé s'il n'y en a aucun d'installé.
/// Idempotente : `running` qui court encore, ou une voix déjà là, ne
/// déclenchent rien. Un téléchargement fini (échoué compris) se relance.
pub fn ensure_default_voice(mlx: bool, running: Option<&Download>) -> Result<Option<Download>> {
    if running.is_some_and(|d| !d.is_finished()) {
        return Ok(None);
    }
    match default_voice_to_install(&available_with(mlx), is_installed) {
        Some(package) => Download::start(package).map(Some),
        None => Ok(None),
    }
}

/// Octets présents sur disque pour ce paquet : fichier principal et
/// compagnons, ou leur `.part` (un téléchargement interrompu occupe aussi de
/// la place).
pub fn installed_bytes_in(root: &Path, package: &ModelPackage) -> u64 {
    std::iter::once(main_path_in(root, package))
        .chain(package.companions.iter().map(|c| companion_path_in(root, package, c)))
        .filter_map(|p| size_of(&p).or_else(|| size_of(&p.with_extension("part"))))
        .sum()
}

pub fn installed_bytes(package: &ModelPackage) -> u64 {
    models_dir().map_or(0, |root| installed_bytes_in(&root, package))
}

/// Vrai si un fichier du paquet, ou un `.part` de lui, traîne sur le disque :
/// de quoi proposer de le retirer même incomplet.
///
/// Un compagnon qu'un autre paquet de la famille, présent, nomme aussi ne
/// compte pas : il n'est pas à ce paquet seul, et `remove_in` le garderait.
pub fn has_files_in(root: &Path, package: &ModelPackage) -> bool {
    let present = |p: &Path| p.exists() || p.with_extension("part").exists();
    present(&main_path_in(root, package))
        || package.companions.iter().any(|c| {
            !shared_with_sibling(root, package, c) && present(&companion_path_in(root, package, c))
        })
}

/// Un autre paquet de la famille nomme ce compagnon et est là : installé, ou
/// en cours de téléchargement (son fichier principal ou son `.part`).
fn shared_with_sibling(root: &Path, package: &ModelPackage, companion: &Companion) -> bool {
    CATALOG.iter().any(|other| {
        let main = main_path_in(root, other);
        other.id != package.id
            && other.family == package.family
            && (main.exists() || main.with_extension("part").exists())
            && other.companions.iter().any(|c| c.dest == companion.dest)
    })
}

pub fn has_files(package: &ModelPackage) -> bool {
    models_dir().is_some_and(|root| has_files_in(&root, package))
}

/// Le dossier d'un paquet Kokoro complet, d'après son fichier principal :
/// celui où se trouvent `config.json` et `voices/`.
pub fn kokoro_dir(model_path: &str) -> Option<PathBuf> {
    let dir = Path::new(model_path).parent()?;
    (Path::new(model_path).is_file() && dir.join("config.json").is_file()).then(|| dir.to_path_buf())
}

pub fn remove(package: &ModelPackage) -> Result<()> {
    let root = models_dir().context(crate::tr!("data directory not found", "répertoire de données introuvable"))?;
    remove_in(&root, package)
}

/// Supprime un paquet. Un compagnon partagé reste tant qu'un autre paquet de
/// la famille, présent, le nomme : retirer le paquet MLX ne casse pas l'int8.
pub fn remove_in(root: &Path, package: &ModelPackage) -> Result<()> {
    let path = main_path_in(root, package);
    // Un paquet interrompu n'a pas forcément tous ses fichiers : l'absence
    // n'arrête pas le nettoyage. Les `.part` partent aussi.
    let mut first_error = None;
    for file in [path.clone(), path.with_extension("part")] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                first_error.get_or_insert_with(|| {
                    anyhow!(crate::tr!(en = "removing {}: {e}", fr = "suppression de {} : {e}", file.display()))
                });
            }
        }
    }
    for companion in package.companions {
        if !shared_with_sibling(root, package, companion) {
            let file = companion_path_in(root, package, companion);
            let _ = std::fs::remove_file(&file);
            let _ = std::fs::remove_file(file.with_extension("part"));
        }
    }
    // Laisse le dossier de famille propre s'il devient vide : les dossiers
    // des compagnons (`speech_tokenizer/`, `text_encoder/`, `voices/`…) d'abord,
    // du plus profond au moins profond, puis la famille. Un dossier non vide
    // (compagnon partagé gardé) ou absent reste tel quel.
    let family = root.join(package.family);
    let mut dirs: Vec<PathBuf> = Vec::new();
    for companion in package.companions {
        let mut dir = companion_path_in(root, package, companion).parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            if d == family || !d.starts_with(&family) {
                break;
            }
            if !dirs.contains(&d) {
                dirs.push(d.clone());
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for dir in &dirs {
        let _ = std::fs::remove_dir(dir);
    }
    let _ = std::fs::remove_dir(&family);
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Espace disque libre du volume des modèles, en octets.
pub fn free_space() -> Option<u64> {
    let dir = models_dir()?;
    // Le répertoire peut ne pas exister encore : on interroge son ancêtre.
    let probe = dir.ancestors().find(|p| p.exists())?;
    free_space_at(probe)
}

#[cfg(unix)]
fn free_space_at(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY : `statvfs` remplit une structure que nous fournissons, et le
    // chemin est une chaîne C valide.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_space_at(_path: &Path) -> Option<u64> {
    None
}

/// État d'un téléchargement, consulté par l'interface à chaque trame.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Running { received: u64, total: u64 },
    Done,
    Failed(String),
    Cancelled,
}

/// Téléchargement en cours, annulable.
pub struct Download {
    pub package: &'static ModelPackage,
    received: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    outcome: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
}

impl Download {
    /// Lance le téléchargement dans un thread dédié : fichier principal puis
    /// compagnons, en reprenant ce qui est déjà là (un transfert coupé par
    /// le réseau ou la fermeture de l'app). Annulé, il ne laisse rien.
    pub fn start(package: &'static ModelPackage) -> Result<Self> {
        let root = models_dir().context(crate::tr!("data directory not found", "répertoire de données introuvable"))?;
        Self::start_in(package, planned_files_in(&root, package), Some(root))
    }

    /// Télécharge ces fichiers (adresse, destination, taille) pour ce paquet.
    /// `root` : le dossier des modèles ; une annulation y retire alors tout
    /// ce que le paquet avait posé (`remove_in` : fichiers, `.part`, dossier
    /// de famille vide ; les compagnons partagés avec un frère restent).
    pub(crate) fn start_in(
        package: &'static ModelPackage,
        files: Vec<(String, PathBuf, u64)>,
        root: Option<PathBuf>,
    ) -> Result<Self> {
        let received = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(package.total_bytes()));
        let outcome = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = (Arc::clone(&received), Arc::clone(&outcome), Arc::clone(&cancel));
        crate::sync::spawn_named("dysflow-telechargement", move || {
            let (received, outcome, cancel) = worker;
            let mut base = 0u64;
            let mut result = Ok(());
            for (url, destination, bytes) in files {
                if size_of(&destination) == Some(bytes) {
                    base += bytes;
                    received.store(base, Ordering::Relaxed);
                    continue;
                }
                result = fetch(&url, &destination, bytes, &received, base, &cancel);
                if result.is_err() || cancel.load(Ordering::Relaxed) {
                    break;
                }
                base += bytes;
            }
            let state = match result {
                Ok(()) if cancel.load(Ordering::Relaxed) => {
                    // Annuler, c'est renoncer : rien ne reste sur le disque.
                    if let Some(root) = &root {
                        let _ = remove_in(root, package);
                    }
                    Progress::Cancelled
                }
                Ok(()) => Progress::Done,
                Err(err) => Progress::Failed(err.to_string()),
            };
            *outcome.locked() = Some(state);
        });

        Ok(Self { package, received, total, outcome, cancel })
    }

    pub fn progress(&self) -> Progress {
        if let Some(state) = self.outcome.locked().clone() {
            return state;
        }
        Progress::Running {
            received: self.received.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }

    /// Un téléchargement sans réseau ni fil, pour les tests.
    #[cfg(test)]
    pub(crate) fn fake(package: &'static ModelPackage, finished: bool) -> Self {
        Self {
            package,
            received: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(package.total_bytes())),
            outcome: Arc::new(Mutex::new(finished.then_some(Progress::Done))),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Un téléchargement fini sur cette issue (échec, annulation…), pour les
    /// tests.
    #[cfg(test)]
    pub(crate) fn fake_ended(package: &'static ModelPackage, ending: Progress) -> Self {
        let download = Self::fake(package, false);
        *download.outcome.locked() = Some(ending);
        download
    }

    /// Vrai si l'arrêt a été demandé.
    #[cfg(test)]
    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.outcome.locked().is_some()
    }
}

/// Télécharge `url` vers `destination`, en reprenant un transfert interrompu.
///
/// L'écriture passe par un fichier `.part` renommé à la fin : un modèle présent
/// dans le répertoire est donc toujours un modèle complet.
///
/// `base` est ce que les fichiers précédents du paquet ont déjà reçu : la
/// progression est celle du paquet entier.
fn fetch(
    url: &str,
    destination: &Path,
    expected: u64,
    received: &AtomicU64,
    base: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    fetch_with(url, destination, expected, received, base, cancel, &|| {})
}

/// `fetch`, avec un point d'accroche juste avant que le `.part` soit créé.
///
/// Entre la première création du dossier et la création du `.part`, une
/// requête réseau s'écoule : un `models::remove` concurrent peut y retirer un
/// dossier de famille devenu vide. Le dossier est donc recréé au dernier
/// moment, et le test d'accroche le retire exactement là.
fn fetch_with(
    url: &str,
    destination: &Path,
    expected: u64,
    received: &AtomicU64,
    base: u64,
    cancel: &AtomicBool,
    before_create: &dyn Fn(),
) -> Result<()> {
    let ensure_parent = || -> Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| crate::tr!(en = "creating {}", fr = "création de {}", parent.display()))?;
        }
        Ok(())
    };
    ensure_parent()?;
    let partial = destination.with_extension("part");
    let already = match resume_from(&partial, expected) {
        // Tout est déjà là : le serveur répondrait 416 à une reprise.
        Resume::Complete => {
            received.store(base + expected, Ordering::Relaxed);
            before_create();
            ensure_parent()?;
            return settle(&partial, destination, expected);
        }
        Resume::At(bytes) => bytes,
        Resume::FromScratch => 0,
    };

    // Un serveur qui ne répond plus ne doit pas figer l'installation : sans
    // délai, `d` restait sans effet et toute autre installation refusée.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();
    let mut request = agent.get(url);
    if already > 0 {
        // Reprend là où le transfert précédent s'est arrêté.
        request = request.set("Range", &format!("bytes={already}-"));
    }
    let response = request.call().map_err(|e| anyhow!("{e}"))?;

    let resuming = response.status() == 206;
    before_create();
    ensure_parent()?;
    let mut file = if resuming {
        received.store(base + already, Ordering::Relaxed);
        std::fs::OpenOptions::new().append(true).open(&partial)?
    } else {
        received.store(base, Ordering::Relaxed);
        std::fs::File::create(&partial)?
    };

    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = if resuming { already } else { 0 };

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Le `.part` est conservé : une reprise repartira de là.
            return Ok(());
        }
        let read = reader.read(&mut buffer).context(crate::tr!("reading the network stream", "lecture du flux réseau"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).context(crate::tr!("writing to disk", "écriture sur disque"))?;
        written += read as u64;
        received.store(base + written, Ordering::Relaxed);
    }
    file.flush()?;
    drop(file);

    settle(&partial, destination, expected)
}

/// Ce qu'un `.part` laissé par un transfert précédent permet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resume {
    /// Rien : il n'y en a pas, ou il ne vient pas du même fichier.
    FromScratch,
    /// Reprendre à cet octet.
    At(u64),
    /// Tout est déjà là.
    Complete,
}

/// Décide de la reprise d'après la taille du `.part` et celle qu'on attend.
///
/// Un `.part` plus long que le fichier annoncé ne vient pas de lui : il est
/// retiré, sans quoi la reprise le prolongerait sans fin.
fn resume_from(partial: &Path, expected: u64) -> Resume {
    let Ok(len) = std::fs::metadata(partial).map(|m| m.len()) else {
        return Resume::FromScratch;
    };
    if expected > 0 && len == expected {
        Resume::Complete
    } else if expected > 0 && len > expected {
        let _ = std::fs::remove_file(partial);
        Resume::FromScratch
    } else if len > 0 {
        Resume::At(len)
    } else {
        Resume::FromScratch
    }
}

/// Met en place un transfert terminé, si sa taille est celle annoncée.
///
/// Un fichier plus court vient d'un disque plein ou d'un serveur qui a coupé
/// court ; un fichier plus long, d'une reprise sur un blob qui a changé entre
/// temps. Dans les deux cas le moteur refuserait de l'ouvrir sur un message
/// illisible, et le `.part` ne peut plus servir de base : il est retiré.
fn settle(partial: &Path, destination: &Path, expected: u64) -> Result<()> {
    let len = std::fs::metadata(partial)
        .with_context(|| crate::tr!(en = "reading {}", fr = "lecture de {}", partial.display()))?
        .len();
    if expected > 0 && len != expected {
        let _ = std::fs::remove_file(partial);
        anyhow::bail!(crate::tr!(
            en = "received {} instead of {}: the download must start over",
            fr = "fichier reçu de {} au lieu de {} : le transfert est à refaire",
            human_size(len),
            human_size(expected),
        ));
    }
    std::fs::rename(partial, destination)
        .with_context(|| crate::tr!(en = "installing {}", fr = "installation de {}", destination.display()))?;
    Ok(())
}

/// Formate un nombre d'octets pour l'affichage, dans la langue de
/// l'interface : « 298,3 Mo » en français, « 298.3 MB » en anglais.
pub fn human_size(bytes: u64) -> String {
    let french = crate::i18n::ui_lang() == crate::i18n::UiLang::Fr;
    let units: [(&str, u64); 3] = if french {
        [("Go", 1 << 30), ("Mo", 1 << 20), ("Ko", 1 << 10)]
    } else {
        [("GB", 1 << 30), ("MB", 1 << 20), ("KB", 1 << 10)]
    };
    for (unit, scale) in units {
        if bytes >= scale {
            let size = format!("{:.1} {unit}", bytes as f64 / scale as f64);
            // Virgule décimale, à la française ; point en anglais.
            return if french { size.replace('.', ",") } else { size };
        }
    }
    if french { format!("{bytes} o") } else { format!("{bytes} B") }
}

/// Une voix intégrée, proposée au choix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: &'static str,
    pub label: Tr,
    /// Langue de la voix ; vide : toutes celles du modèle.
    pub language: &'static str,
}

/// Voix proposées pour une famille donnée.
///
/// Vide quand la famille n'expose pas de voix nommées : le champ redevient
/// alors libre, pour y mettre un chemin de WAV à cloner.
pub fn voices_for_family(family: &str) -> &'static [VoiceOption] {
    match family {
        "kokoro" => KOKORO_VOICES,
        "qwen3_tts_06b_base_8bit" => QWEN3_BASE_VOICES,
        "magpie_tts_357m_8bit" => MAGPIE_VOICES,
        _ => &[],
    }
}

/// Voix de Kokoro : toutes celles du paquet, plus `p-fenrir60` livrée avec
/// Dysflow (16 oct. 2026 : une seule voix pour tout, choisie librement).
///
/// `language` dit l'accent de la voix, pas ce qu'elle a le droit de lire :
/// la prononciation suit la langue du texte. La première d'une langue est
/// sa voix par défaut.
const KOKORO_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "p-fenrir60", label: Tr::new("p-fenrir60 · French, recommended", "p-fenrir60 · française, recommandée"), language: "fr" },
    VoiceOption { id: "ff_siwis", label: Tr::new("ff_siwis · French, female", "ff_siwis · française, femme"), language: "fr" },
    VoiceOption { id: "af_bella", label: Tr::new("af_bella · American, female, recommended", "af_bella · américaine, femme, recommandée"), language: "en" },
    VoiceOption { id: "af_heart", label: Tr::new("af_heart · American, female", "af_heart · américaine, femme"), language: "en" },
    VoiceOption { id: "af_alloy", label: Tr::new("af_alloy · American, female", "af_alloy · américaine, femme"), language: "en" },
    VoiceOption { id: "af_aoede", label: Tr::new("af_aoede · American, female", "af_aoede · américaine, femme"), language: "en" },
    VoiceOption { id: "af_jessica", label: Tr::new("af_jessica · American, female", "af_jessica · américaine, femme"), language: "en" },
    VoiceOption { id: "af_kore", label: Tr::new("af_kore · American, female", "af_kore · américaine, femme"), language: "en" },
    VoiceOption { id: "af_nicole", label: Tr::new("af_nicole · American, female", "af_nicole · américaine, femme"), language: "en" },
    VoiceOption { id: "af_nova", label: Tr::new("af_nova · American, female", "af_nova · américaine, femme"), language: "en" },
    VoiceOption { id: "af_river", label: Tr::new("af_river · American, female", "af_river · américaine, femme"), language: "en" },
    VoiceOption { id: "af_sarah", label: Tr::new("af_sarah · American, female", "af_sarah · américaine, femme"), language: "en" },
    VoiceOption { id: "af_sky", label: Tr::new("af_sky · American, female", "af_sky · américaine, femme"), language: "en" },
    VoiceOption { id: "am_fenrir", label: Tr::new("am_fenrir · American, male", "am_fenrir · américaine, homme"), language: "en" },
    VoiceOption { id: "am_adam", label: Tr::new("am_adam · American, male", "am_adam · américaine, homme"), language: "en" },
    VoiceOption { id: "am_echo", label: Tr::new("am_echo · American, male", "am_echo · américaine, homme"), language: "en" },
    VoiceOption { id: "am_eric", label: Tr::new("am_eric · American, male", "am_eric · américaine, homme"), language: "en" },
    VoiceOption { id: "am_liam", label: Tr::new("am_liam · American, male", "am_liam · américaine, homme"), language: "en" },
    VoiceOption { id: "am_michael", label: Tr::new("am_michael · American, male", "am_michael · américaine, homme"), language: "en" },
    VoiceOption { id: "am_onyx", label: Tr::new("am_onyx · American, male", "am_onyx · américaine, homme"), language: "en" },
    VoiceOption { id: "am_puck", label: Tr::new("am_puck · American, male", "am_puck · américaine, homme"), language: "en" },
    VoiceOption { id: "am_santa", label: Tr::new("am_santa · American, male", "am_santa · américaine, homme"), language: "en" },
    VoiceOption { id: "bf_alice", label: Tr::new("bf_alice · British, female", "bf_alice · britannique, femme"), language: "en" },
    VoiceOption { id: "bf_emma", label: Tr::new("bf_emma · British, female", "bf_emma · britannique, femme"), language: "en" },
    VoiceOption { id: "bf_isabella", label: Tr::new("bf_isabella · British, female", "bf_isabella · britannique, femme"), language: "en" },
    VoiceOption { id: "bf_lily", label: Tr::new("bf_lily · British, female", "bf_lily · britannique, femme"), language: "en" },
    VoiceOption { id: "bm_daniel", label: Tr::new("bm_daniel · British, male", "bm_daniel · britannique, homme"), language: "en" },
    VoiceOption { id: "bm_fable", label: Tr::new("bm_fable · British, male", "bm_fable · britannique, homme"), language: "en" },
    VoiceOption { id: "bm_george", label: Tr::new("bm_george · British, male", "bm_george · britannique, homme"), language: "en" },
    VoiceOption { id: "bm_lewis", label: Tr::new("bm_lewis · British, male", "bm_lewis · britannique, homme"), language: "en" },
];

const QWEN3_BASE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "siwis", label: Tr::new("siwis · French, female, recommended", "siwis · française, femme, recommandée"), language: "fr" },
];

/// Les cinq voix du paquet MLX, dans l'ordre de `baked_speaker_names`
/// (relevé §3.d) ; `MagpieSpeaker(named:)` les reconnaît.
const MAGPIE_VOICES: &[VoiceOption] = &[
    VoiceOption { id: "sofia", label: Tr::new("sofia · American, female, recommended", "sofia · américaine, femme, recommandée"), language: "en" },
    VoiceOption { id: "aria", label: Tr::new("aria · American, female", "aria · américaine, femme"), language: "en" },
    VoiceOption { id: "jason", label: Tr::new("jason · American, male", "jason · américaine, homme"), language: "en" },
    VoiceOption { id: "leo", label: Tr::new("leo · American, male", "leo · américaine, homme"), language: "en" },
    VoiceOption { id: "john_van_stan", label: Tr::new("john_van_stan · American, male (LibriVox)", "john_van_stan · américaine, homme (LibriVox)"), language: "en" },
];

/// Les familles lues par un moteur à texte (lot 3) : une session, la voix et
/// la langue honorées.
fn reads_text(family: &str) -> bool {
    CATALOG
        .iter()
        .any(|p| p.family == family && matches!(p.runtime, Runtime::Qwen3TtsMlx | Runtime::MagpieMlx))
}

/// Sessions de synthèse qu'une famille demande pour lire d'un trait.
///
/// Une session pèse environ 460 Mo — deux fois le modèle — et s'ouvre après
/// les autres, derrière le verrou de cycle de vie. En ouvrir plus que
/// nécessaire coûte donc de la mémoire *et* retarde le premier son.
pub fn sessions_for_family(family: &str) -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    match family {
        // Kokoro, Qwen3 et Magpie calculent en série sous un seul verrou MLX :
        // une session.
        f if f == "kokoro" || reads_text(f) => 1,
        // Les familles inconnues sont supposées lentes et parallélisables, la
        // synthèse en avance valant mieux qu'un silence ; jamais plus d'une
        // session pour deux cœurs, pour que la lecture garde les siens.
        _ => (cores / 3).clamp(2, 4).min((cores / 2).max(1)),
    }
}

/// Réglages de synthèse réellement honorés par une famille.
///
/// Toutes les familles n'en lisent qu'une partie. Afficher un réglage sans
/// effet donnerait l'illusion d'agir sur la synthèse — et l'aperçu, lui, ne
/// ment pas : mieux vaut dire d'avance ce qui restera sans suite.
pub fn honoured_options(family: &str) -> &'static [&'static str] {
    match family {
        // Kokoro, Qwen3, Magpie : la voix et la langue ; la vitesse
        // s'applique au son.
        f if f == "kokoro" || reads_text(f) => &["voice", "language"],
        // Famille inconnue : on n'affirme rien, tout est présenté comme possible.
        _ => &[],
    }
}

/// Voix par défaut d'une famille, quand elle en nomme.
pub fn default_voice(family: &str) -> &'static str {
    voices_for_family(family).first().map_or("", |v| v.id)
}

/// Vrai quand la famille nomme ses voix : la voix se range alors dans
/// `voice.per_language`, une pour tout.
pub fn voices_follow_language(family: &str) -> bool {
    !voices_for_family(family).is_empty()
}

fn language_code(language: &str) -> &str {
    crate::text::language::Lang::parse(language).map_or(language, |l| l.code())
}

/// Voix par défaut d'une famille dans une langue ; sa voix par défaut tout
/// court si aucune n'est rangée sous cette langue.
pub fn default_voice_for_language(family: &str, language: &str) -> &'static str {
    let code = language_code(language);
    voices_for_family(family)
        .iter()
        .find(|v| v.language == code)
        .map_or_else(|| default_voice(family), |v| v.id)
}

/// La voix qui parle `language` : celle de la configuration si la famille la
/// connaît (quelle que soit son accent : une voix pour tout), sinon la voix
/// par défaut de la langue — sans réécrire la configuration.
pub fn voice_for_language(family: &str, language: &str, configured: Option<&str>) -> &'static str {
    let code = language_code(language);
    configured
        .map(str::trim)
        .and_then(|c| voices_for_family(family).iter().find(|v| v.id == c))
        .map_or_else(|| default_voice_for_language(family, code), |v| v.id)
}

/// Vrai si la famille prend en compte ce réglage.
pub fn honours(family: &str, option: &str) -> bool {
    let honoured = honoured_options(family);
    honoured.is_empty() || honoured.contains(&option)
}

/// Langues proposées quand la famille n'est pas au catalogue : les sept que
/// le terminal proposait.
pub const FALLBACK_LANGUAGES: &[&str] = &["fr", "en", "de", "es", "it", "pt", "nl"];

/// Langues qu'une famille sait parler : celles de son paquet au catalogue.
///
/// Toutes les variantes d'une famille partagent la même liste ; le catalogue
/// entier est consulté, pas seulement ce que ce binaire sait charger, pour
/// que l'écran dise juste même compilé sans Kokoro.
pub fn languages_for_family(family: &str) -> Vec<&'static str> {
    CATALOG
        .iter()
        .find(|p| p.family == family)
        .map(|p| p.languages.to_vec())
        .unwrap_or_else(|| FALLBACK_LANGUAGES.to_vec())
}

/// Ce que le catalogue décide pour une configuration : la voix de chaque
/// langue et le nombre de sessions.
impl crate::config::Config {
    /// Une voix pour tout (16 oct. 2026) : la voix française de Kokoro devient
    /// celle de chaque langue. Vrai si quelque chose a changé.
    pub fn unify_voices(&mut self) -> bool {
        if !voices_follow_language(&self.engine.family) {
            return false;
        }
        let voice = self
            .voice
            .per_language
            .get("fr")
            .cloned()
            .unwrap_or_else(|| default_voice_for_language(&self.engine.family, "fr").to_string());
        let mut changed = false;
        for language in ["fr", "en"] {
            if self.voice.per_language.get(language) != Some(&voice) {
                self.voice.per_language.insert(language.to_string(), voice.clone());
                changed = true;
            }
        }
        changed
    }

    /// Fait suivre la voix quand la famille change.
    ///
    /// Seul Kokoro nomme ses voix, une par langue : `voice.voice` reste celle
    /// de l'ancien modèle, pour qui voudra y revenir, et seules les langues
    /// sans voix reçoivent la leur. Une voix retirée du catalogue n'est pas
    /// réécrite : `tts_params_in` la remplace par la voix recommandée de sa
    /// langue (règle des valeurs non listées). Les autres familles ne nomment
    /// aucune voix : rien à aligner. Renvoie vrai si quelque chose a changé.
    pub fn align_voice_with_family(&mut self) -> bool {
        let family = self.engine.family.clone();
        if !voices_follow_language(&family) {
            return false;
        }
        let mut changed = false;
        for language in ["fr", "en"] {
            if !self.voice.per_language.contains_key(language) {
                let voice = default_voice_for_language(&family, language);
                self.voice.per_language.insert(language.to_string(), voice.to_string());
                changed = true;
            }
        }
        changed
    }

    /// Sessions de synthèse réellement ouvertes.
    ///
    /// Un nombre posé à la main l'emporte ; `0` s'en remet à la famille.
    pub fn sessions(&self) -> u32 {
        match self.engine.sessions {
            0 => sessions_for_family(&self.engine.family),
            n => n,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn voices_are_unified_on_the_french_one() {
        let mut c = crate::config::Config::default();
        c.engine.family = "kokoro".into();
        c.voice.per_language.insert("fr".into(), "p-fenrir60".into());
        c.voice.per_language.insert("en".into(), "af_bella".into());
        assert!(c.unify_voices());
        assert_eq!(c.voice.per_language["en"], "p-fenrir60");
        assert!(!c.unify_voices(), "déjà unifiées : rien à écrire");
    }


    #[test]
    fn every_kokoro_voice_of_the_package_is_offered() {
        let offered: Vec<&str> = voices_for_family("kokoro").iter().map(|v| v.id).collect();
        assert_eq!(offered[0], "p-fenrir60", "la recommandée en tête");
        for companion in KOKORO_SHARED.iter().filter(|c| c.dest.starts_with("voices/")) {
            let id = companion.dest.trim_start_matches("voices/").trim_end_matches(".safetensors");
            assert!(offered.contains(&id), "{id} manque au choix");
        }
        assert_eq!(offered.len(), 30);
    }

    #[test]
    fn a_voice_reads_any_language_of_its_model() {
        // Une seule voix pour tout : af_bella lit aussi le français.
        assert_eq!(voice_for_language("kokoro", "fr", Some("af_bella")), "af_bella");
        assert_eq!(voice_for_language("kokoro", "en", Some("p-fenrir60")), "p-fenrir60");
        // Une voix inconnue retombe sur la recommandée de la langue.
        assert_eq!(voice_for_language("kokoro", "fr", Some("inconnue")), "p-fenrir60");
    }

    #[test]
    fn kokoro_reads_aloud() {
        assert!(CATALOG.iter().filter(|p| p.family == "kokoro").all(|p| p.kind == Kind::Speech));
    }
    use super::*;

    #[test]
    fn languages_follow_the_family() {
        assert_eq!(languages_for_family("kokoro"), ["fr", "en"]);
        // Famille inconnue : les sept que le terminal proposait.
        assert_eq!(languages_for_family(""), FALLBACK_LANGUAGES.to_vec());
    }

    /// Retrouve une entrée par son identifiant, plutôt que par sa position :
    /// l'ordre du catalogue suit la recommandation et change avec elle.
    fn package(id: &str) -> &'static ModelPackage {
        CATALOG
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} absent du catalogue"))
    }

    #[test]
    fn kokoro_gets_a_single_session() {
        assert_eq!(sessions_for_family("kokoro"), 1);
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
        let unknown = sessions_for_family("inconnue");
        assert!(unknown >= 1 && unknown <= (cores / 2).max(1), "{unknown} sessions pour {cores} cœurs");
        assert!(cores < 4 || unknown >= 2);
    }

    #[test]
    fn urls_follow_the_huggingface_layout() {
        assert_eq!(
            package("kokoro_82m_mlx").url(),
            "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/kokoro-v1_0.safetensors"
        );
        assert_eq!(
            package("kokoro_82m_int8").url(),
            "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX-timestamped/resolve/dd4401a9add81ac692d20e240d22ec9dda82cc29/onnx/model_quantized.onnx"
        );
    }

    #[test]
    fn file_name_is_extracted_from_the_repo_path() {
        assert_eq!(package("kokoro_82m_mlx").file_name(), "kokoro-v1_0.safetensors");
        assert_eq!(package("kokoro_82m_int8").file_name(), "model_quantized.onnx");
    }

    #[test]
    fn every_reading_model_speaks_french() {
        // Le lecteur s'adresse d'abord à un usage francophone. La dictée peut
        // proposer un modèle anglais seulement (Parakeet v2) : la
        // bibliothèque des modèles le filtre par langue.
        for package in CATALOG.iter().filter(|p| p.kind == Kind::Speech) {
            assert!(package.speaks("fr"), "{} ne parle pas français", package.id);
            assert!(package.speaks("fr-FR"), "variante régionale rejetée");
        }
    }

    #[test]
    fn kokoro_is_the_recommended_voice_wherever_it_runs() {
        let speech = |p: &&ModelPackage| p.kind == Kind::Speech;
        assert!(CATALOG.iter().filter(speech).filter(|p| p.recommended).all(|p| p.family == "kokoro"));
        for mlx in [true, false] {
            for kind in [Kind::Speech, Kind::Dictation] {
                let recommended = available_with(mlx).into_iter().filter(|p| p.kind == kind && p.recommended).count();
                assert!(recommended <= 1, "deux paquets recommandés côte à côte (mlx = {mlx}, {kind:?})");
            }
        }
    }

    /// Kokoro (MLX ou int8), puis les trois moteurs à texte du lot 3. Le
    /// terminal ne voit toujours que Kokoro.
    #[test]
    fn the_speech_models_are_kokoro_qwen3_and_magpie() {
        let ids: Vec<&str> = CATALOG.iter().filter(|p| p.kind == Kind::Speech).map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                "kokoro_82m_mlx",
                "kokoro_82m_int8",
                "qwen3_tts_06b_base_8bit",
                "magpie_tts_357m_8bit"
            ]
        );
        assert!(available().iter().filter(|p| p.kind == Kind::Speech).all(|p| p.family == "kokoro"));
        assert_eq!(
            available_with(true).iter().filter(|p| matches!(p.runtime, Runtime::Qwen3TtsMlx | Runtime::MagpieMlx)).count(),
            2
        );
    }

    const TEXT_MODELS: [&str; 2] = ["qwen3_tts_06b_base_8bit", "magpie_tts_357m_8bit"];

    /// Tailles relevées sur les dépôts aux révisions figées (relevé §2.a,
    /// §2.b, §3.d) : seuls les fichiers que les chargeurs lisent.
    #[test]
    fn text_models_have_their_own_folder_and_only_the_files_they_read() {
        for (id, total) in [
            ("qwen3_tts_06b_base_8bit", 1_991_218_180),
            ("magpie_tts_357m_8bit", 301_589_611 + 32),
        ] {
            let p = package(id);
            assert_eq!(p.family, p.id, "{id} : un dossier par paquet");
            assert_eq!(p.total_bytes(), total, "{id}");
            assert!(p.companions.iter().all(|c| c.repo == p.repo && c.revision == p.revision), "{id}");
            assert!(p.companions.iter().all(|c| c.dest == c.file), "{id} : même arborescence que le dépôt");
            assert!(p.description.get().contains("Suivi des mots approché"), "{id}");
            assert!(!p.recommended, "{id}");
        }
        let magpie = package("magpie_tts_357m_8bit");
        assert!(!magpie.companions.iter().any(|c| c.file.starts_with("decoder_step/")), "jamais lu par MLX");
        let qwen = package("qwen3_tts_06b_base_8bit");
        assert!(qwen.companions.iter().any(|c| c.dest == "speech_tokenizer/model.safetensors" && c.bytes == 682_293_092));
        assert!(!qwen.companions.iter().any(|c| c.file.ends_with("index.json")), "jamais lu");
    }

    #[test]
    fn text_models_read_one_sentence_at_a_time_with_their_voice_and_language() {
        for id in TEXT_MODELS {
            assert_eq!(sessions_for_family(id), 1, "{id}");
            assert_eq!(honoured_options(id), &["voice", "language"], "{id}");
            assert!(voices_follow_language(id), "{id}");
        }
    }

    #[test]
    fn qwen3_and_magpie_name_their_voices() {
        let ids = |family: &str| voices_for_family(family).iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(ids("qwen3_tts_06b_base_8bit"), ["siwis"]);
        assert_eq!(ids("magpie_tts_357m_8bit"), ["sofia", "aria", "jason", "leo", "john_van_stan"]);
        assert_eq!(default_voice_for_language("magpie_tts_357m_8bit", "fr"), "sofia");
        assert_eq!(default_voice_for_language("qwen3_tts_06b_base_8bit", "en"), "siwis");
    }

    /// Spec : voix choisie absente du nouveau modèle → sa voix par défaut,
    /// sans message (et sans réécrire la configuration).
    #[test]
    fn a_voice_of_another_model_gives_the_default() {
        assert_eq!(voice_for_language("qwen3_tts_06b_base_8bit", "fr", Some("p-fenrir60")), "siwis");
        assert_eq!(voice_for_language("magpie_tts_357m_8bit", "fr", Some("Thomas")), "sofia");
        assert_eq!(voice_for_language("magpie_tts_357m_8bit", "en", Some("leo")), "leo");
    }

    #[test]
    fn the_catalogue_reads_in_english_without_a_word_of_french() {
        use crate::i18n::{with_ui_lang, UiLang};
        with_ui_lang(UiLang::En, || {
            for p in CATALOG {
                for (name, text) in [
                    ("display_name", p.display_name.get()),
                    ("description", p.description.get()),
                    ("precision", p.precision.get()),
                    ("languages_label", p.languages_label.get()),
                    ("pace", p.pace.get()),
                    ("voices_label", p.voices_label.get()),
                    ("licenses", p.licenses.get()),
                ] {
                    for french in ["é", "è", "à", " voix", " langues"] {
                        assert!(!text.contains(french), "{} {name} : {french} dans {text:?}", p.id);
                    }
                }
            }
            for v in voices_for_family("kokoro") {
                let label = v.label.get();
                assert!(!label.contains("é") && !label.contains("femme"), "{label}");
            }
        });
    }

    #[test]
    fn the_catalogue_keeps_its_french() {
        use crate::i18n::{with_ui_lang, UiLang};
        with_ui_lang(UiLang::Fr, || {
            assert_eq!(
                package("kokoro_82m_mlx").description.get(),
                "Voix naturelle et légère, calculée par la puce graphique."
            );
        });
    }

    #[test]
    fn the_measured_models_say_so_in_their_pace() {
        assert_eq!(
            package("qwen3_tts_06b_base_8bit").pace.get(),
            "Lourd : lecture hachée au-delà de 1× (occupé 76 % du temps à 1× sur M5)."
        );
        assert_eq!(
            package("magpie_tts_357m_8bit").pace.get(),
            "Lourd : lecture hachée au-delà de 1× (occupé 69 % du temps à 1× sur M5)."
        );
    }

    #[test]
    fn magpie_is_installed_with_its_root_config() {
        let Some(path) = install_path(package("magpie_tts_357m_8bit")) else { return };
        assert!(path.ends_with("magpie_tts_357m_8bit/config.json"), "{path:?}");
    }

    #[test]
    fn dictation_packages_each_have_their_own_folder() {
        let dictation: Vec<&ModelPackage> = CATALOG.iter().filter(|p| p.kind == Kind::Dictation).collect();
        assert_eq!(dictation.len(), 12);
        for p in &dictation {
            assert_eq!(p.family, p.id, "{} : un dossier par paquet", p.id);
        }
    }

    #[test]
    fn dictation_is_only_offered_to_the_app() {
        assert!(available_with(false).iter().all(|p| p.kind == Kind::Speech));
        assert_eq!(available_with(true).iter().filter(|p| p.kind == Kind::Dictation).count(), 12);
    }

    #[test]
    fn a_dictation_model_is_never_the_first_voice() {
        // Même installé, un modèle de dictée ne lit pas.
        assert!(first_installed_with(true).is_none_or(|p| p.kind == Kind::Speech));
    }

    #[test]
    fn whisper_reads_its_vocabulary_from_tiktoken() {
        let p = CATALOG.iter().find(|p| p.id == "whisper_tiny_4bit").unwrap();
        assert!(p.companions.iter().any(|c| c.dest == "multilingual.tiktoken"));
        assert!(p.companions.iter().any(|c| c.dest == "config.json"));
    }

    #[test]
    fn dictation_packages_are_pinned_to_their_own_repo() {
        let parakeet = package("parakeet_tdt_v3");
        assert_eq!(parakeet.total_bytes(), 908_392_684 + 318_494);
        assert!(parakeet.recommended);
        for p in CATALOG.iter().filter(|p| p.kind == Kind::Dictation) {
            assert_eq!(p.revision.len(), 40, "{}", p.id);
            assert!(p.companions.iter().all(|c| c.repo == p.repo && c.revision == p.revision), "{}", p.id);
            let expected = if p.runtime == Runtime::PhononMlx { "phonon-2.bps.tar.zst" } else { "model.safetensors" };
            assert_eq!(p.file, expected, "{}", p.id);
        }
    }

    /// Phonon-2 : une seule archive (poids et `config.json`), épinglée, que
    /// l'app lit sans la décompresser sur le disque.
    #[test]
    fn phonon_is_a_single_pinned_archive() {
        let phonon = package("phonon_2");
        assert_eq!(phonon.total_bytes(), 163_515_201);
        assert!(phonon.companions.is_empty());
        assert!(!phonon.recommended, "Parakeet v3 reste le recommandé : Phonon-2 vise l'anglais");
        assert!(phonon.speaks("en"));
        assert!(available_with(true).iter().any(|p| p.id == "phonon_2"));
        assert!(!available_with(false).iter().any(|p| p.id == "phonon_2"));
    }

    /// Lot 4 : cinq modèles de plus, épinglés, avec ce que leur chargeur lit
    /// (tailles relevées sur Hugging Face le 19 oct. 2026).
    #[test]
    fn lot_4_dictation_packages_carry_what_their_loader_reads() {
        let v2 = package("parakeet_tdt_v2");
        assert_eq!(v2.runtime, Runtime::ParakeetMlx, "même moteur que v3");
        assert_eq!(v2.total_bytes(), 2_471_559_904 + 36_176);
        assert!(v2.speaks("en") && !v2.speaks("fr"), "Parakeet v2 ne connaît que l'anglais");

        let nemotron = package("nemotron_35_asr_streaming_8bit");
        assert_eq!(nemotron.runtime, Runtime::NemotronMlx);
        assert_eq!(nemotron.total_bytes(), 755_598_923 + 159_605);

        for (id, weights) in [("qwen3_asr_06b_8bit", 1_006_229_426), ("qwen3_asr_17b_4bit", 1_603_081_617)] {
            let qwen = package(id);
            assert_eq!(qwen.runtime, Runtime::Qwen3AsrMlx, "{id}");
            assert_eq!(qwen.bytes, weights, "{id}");
            let files: Vec<&str> = qwen.companions.iter().map(|c| c.dest).collect();
            assert_eq!(files, ["config.json", "vocab.json", "merges.txt", "tokenizer_config.json"], "{id}");
        }

        let cohere = package("cohere_transcribe_8bit");
        assert_eq!(cohere.runtime, Runtime::CohereMlx);
        // Vraiment quantifié : le miroir mlx-community (4,13 Go de BF16) n'est pas celui-là.
        assert_eq!(cohere.repo, "beshkenadze/cohere-transcribe-03-2026-mlx-8bit");
        assert_eq!(cohere.total_bytes(), 2_418_031_831 + 4_336 + 492_827 + 48_141);
        assert!(cohere.companions.iter().any(|c| c.dest == "tokenizer.model"));

        for p in [v2, nemotron, package("qwen3_asr_06b_8bit"), package("qwen3_asr_17b_4bit"), cohere] {
            assert!(!p.recommended, "{} : Parakeet v3 reste le recommandé", p.id);
            assert!(available_with(true).iter().any(|q| q.id == p.id), "{}", p.id);
            assert!(!available_with(false).iter().any(|q| q.id == p.id), "{} : pas dans le terminal", p.id);
        }
    }

    #[test]
    fn the_mlx_package_needs_the_app_and_never_sits_next_to_onnx() {
        let with_mlx: Vec<&str> = available_with(true).iter().map(|p| p.id).collect();
        assert!(with_mlx.contains(&"kokoro_82m_mlx"));
        assert!(!with_mlx.contains(&"kokoro_82m_int8"), "pas d'ORT dans l'app");
        let without: Vec<&str> = available_with(false).iter().map(|p| p.id).collect();
        assert!(!without.contains(&"kokoro_82m_mlx"));
        assert_eq!(without.contains(&"kokoro_82m_int8"), cfg!(feature = "kokoro-onnx"));
    }

    #[test]
    fn kokoro_packages_are_pinned_and_carry_their_voices() {
        let mlx = package("kokoro_82m_mlx");
        assert_eq!(mlx.revision, "a71e4d38b236d968966a2002c4c895dbd12b1c3c");
        assert_eq!(mlx.bytes, 327_115_152);
        let int8 = package("kokoro_82m_int8");
        assert_eq!(int8.revision, "dd4401a9add81ac692d20e240d22ec9dda82cc29");
        assert_eq!(int8.file, "onnx/model_quantized.onnx");
        assert_eq!(int8.bytes, 92_361_055);
        // config.json et 29 voix : ff_siwis et les 28 anglaises.
        assert_eq!(mlx.companions.len(), 30);
        assert_eq!(mlx.companions, int8.companions, "voix et configuration communes");
        assert_eq!(mlx.total_bytes(), 327_115_152 + 2_351 + 29 * 522_320);
        assert!(mlx.companions.iter().any(|c| c.dest == "voices/af_bella.safetensors"));
        assert!(mlx.companions.iter().all(|c| c.repo == "mlx-community/Kokoro-82M-bf16"));
        assert_eq!(
            mlx.companions[0].url(),
            "https://huggingface.co/mlx-community/Kokoro-82M-bf16/resolve/a71e4d38b236d968966a2002c4c895dbd12b1c3c/config.json"
        );
    }

    #[test]
    fn kokoro_reads_only_its_voice_and_language() {
        for ignored in ["pitch_shift", "energy", "emotion", "temperature", "seed", "instruction"] {
            assert!(!honours("kokoro", ignored), "{ignored}");
        }
        assert!(honours("kokoro", "voice") && honours("kokoro", "language"));
        assert_eq!(sessions_for_family("kokoro"), 1);
        assert_eq!(languages_for_family("kokoro"), ["fr", "en"]);
    }

    /// Crée les fichiers d'un paquet, creux (taille annoncée, aucun octet
    /// écrit) : 327 Mo ne coûtent rien sur APFS ni ext4.
    fn sparse(root: &Path, package: &ModelPackage, with_companions: bool) {
        let mut files = vec![(main_path_in(root, package), package.bytes)];
        if with_companions {
            files.extend(package.companions.iter().map(|c| (companion_path_in(root, package, c), c.bytes)));
        }
        for (path, bytes) in files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        }
    }

    #[test]
    fn a_package_is_installed_only_when_every_file_is_there() {
        let root = scratch("complet-kokoro");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, false);
        assert!(!is_installed_in(&root, mlx), "voix absentes");
        sparse(&root, mlx, true);
        assert!(is_installed_in(&root, mlx));
        let voice = companion_path_in(&root, mlx, &mlx.companions[3]);
        std::fs::File::create(&voice).unwrap().set_len(10).unwrap();
        assert!(!is_installed_in(&root, mlx), "une voix tronquée");
        assert!(is_truncated_in(&root, mlx));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_truncated_main_file_is_not_installed_and_the_migration_does_not_switch() {
        use crate::reader::migration::{plan, Step};
        let root = scratch("principal-tronque");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, true);
        std::fs::File::create(main_path_in(&root, mlx)).unwrap().set_len(1_000).unwrap();
        assert!(!is_installed_in(&root, mlx), "le modèle principal est tronqué");
        assert!(is_truncated_in(&root, mlx));
        let mut config = crate::config::Config::default();
        config.engine.family = "supertonic".into();
        let step = plan(&config, &[mlx], |p| is_installed_in(&root, p));
        assert_eq!(step, Step::Offer(mlx), "proposé, pas basculé sur un fichier tronqué");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_one_kokoro_package_keeps_what_the_other_needs() {
        let root = scratch("partage-kokoro");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, mlx, true);
        sparse(&root, int8, true);
        remove_in(&root, mlx).unwrap();
        assert!(!main_path_in(&root, mlx).exists());
        assert!(is_installed_in(&root, int8), "voix et configuration restent pour l'autre");
        remove_in(&root, int8).unwrap();
        assert!(!companion_path_in(&root, int8, &int8.companions[0]).exists(), "plus personne n'en a besoin");
        assert!(!root.join("kokoro").exists(), "dossier de famille retiré");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_kokoro_dir_is_one_with_its_configuration() {
        let root = scratch("dossier-kokoro");
        let main = root.join("kokoro").join("kokoro-v1_0.safetensors");
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        std::fs::write(&main, b"x").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), None, "config.json manque");
        std::fs::write(root.join("kokoro").join("config.json"), b"{}").unwrap();
        assert_eq!(kokoro_dir(&main.to_string_lossy()), Some(root.join("kokoro")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_paths_are_scoped_by_family() {
        // Deux familles peuvent nommer leur fichier pareil : le répertoire les
        // sépare.
        let Some(path) = install_path(package("kokoro_82m_mlx")) else { return };
        assert!(path.ends_with("kokoro/kokoro-v1_0.safetensors"), "{path:?}");
        let Some(path) = install_path(package("kokoro_82m_int8")) else { return };
        assert!(path.ends_with("kokoro/model_quantized.onnx"), "{path:?}");
    }

    #[test]
    fn every_catalogued_family_names_its_voices() {
        // C'est la promesse du catalogue : aucun modèle n'exige qu'on lui
        // fabrique une voix avant de pouvoir l'écouter.
        for package in CATALOG.iter().filter(|p| p.kind == Kind::Speech) {
            assert!(
                !voices_for_family(package.family).is_empty(),
                "{} n'expose aucune voix nommée",
                package.family
            );
        }
    }

    #[test]
    fn sizes_are_formatted_readably() {
        assert_eq!(human_size(512), "512 o");
        assert_eq!(human_size(2 * 1024), "2,0 Ko");
        assert_eq!(human_size(312_784_196), "298,3 Mo");
        assert_eq!(human_size(3 * (1 << 30)), "3,0 Go");
        crate::i18n::with_ui_lang(crate::i18n::UiLang::En, || {
            assert_eq!(human_size(512), "512 B");
            assert_eq!(human_size(312_784_196), "298.3 MB");
            assert_eq!(human_size(3 * (1 << 30)), "3.0 GB");
        });
    }

    #[test]
    fn unknown_languages_are_rejected() {
        assert!(!CATALOG[0].speaks("zz"));
    }

    #[test]
    fn unknown_families_offer_no_named_voice() {
        assert!(voices_for_family("inconnue").is_empty());
    }

    #[test]
    fn setting_support_is_reported_per_family() {
        // Kokoro ignore la température : il ne faut pas la présenter
        // comme agissante.
        assert!(!honours("kokoro", "temperature"));
        // Famille inconnue : aucune affirmation, tout reste proposé.
        assert!(honours("inconnue", "temperature"));
    }

    #[test]
    fn no_two_packages_of_a_family_are_the_same_file() {
        // « Q8_0 » et « origine » de Supertonic pointaient le même blob sur le
        // dépôt — même taille, même empreinte : deux entrées pour 454 Mo
        // téléchargés deux fois.
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert!(
                    !(a.family == b.family && a.bytes == b.bytes),
                    "{} et {} sont le même fichier",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// Un serveur local qui répond une fois `body`, sans rien d'externe.
    fn serve_once(body: &'static [u8]) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        url
    }

    #[test]
    fn a_family_dir_removed_during_the_request_does_not_fail_the_download() {
        // `remove` retire un dossier de famille vide pendant que la requête
        // réseau est en vol : le `.part` doit quand même pouvoir être créé,
        // pour le fichier principal comme pour un compagnon dans `voices/`.
        for relative in ["modele.bin", "voices/voix.bin"] {
            let root = scratch("course-dossier");
            let family = root.join("famille");
            let destination = family.join(relative);
            let url = serve_once(b"0123456789");
            let received = AtomicU64::new(0);
            let cancel = AtomicBool::new(false);
            fetch_with(&url, &destination, 10, &received, 0, &cancel, &|| {
                std::fs::remove_dir_all(&family).unwrap();
            })
            .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"0123456789");
            assert_eq!(received.load(Ordering::Relaxed), 10);
            std::fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn a_later_file_reports_progress_after_the_earlier_ones() {
        let root = scratch("decalage");
        let destination = root.join("f").join("deux.bin");
        let url = serve_once(b"abcde");
        let received = AtomicU64::new(0);
        fetch(&url, &destination, 5, &received, 100, &AtomicBool::new(false)).unwrap();
        assert_eq!(received.load(Ordering::Relaxed), 105);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_short_companion_shows_in_the_installed_bytes() {
        let root = scratch("octets-presents");
        let mlx = package("kokoro_82m_mlx");
        sparse(&root, mlx, true);
        assert_eq!(installed_bytes_in(&root, mlx), mlx.total_bytes());
        let voice = companion_path_in(&root, mlx, &mlx.companions[3]);
        std::fs::File::create(&voice).unwrap().set_len(10).unwrap();
        assert_eq!(installed_bytes_in(&root, mlx), mlx.total_bytes() - mlx.companions[3].bytes + 10);
        assert!(is_truncated_in(&root, mlx));
        std::fs::remove_dir_all(&root).ok();
    }

    /// Un téléchargement interrompu occupe la place de ses `.part` : elle se
    /// compte, pour dire ce que « Supprimer » libère.
    #[test]
    fn the_part_files_count_in_the_bytes_on_disk() {
        let root = scratch("octets-part");
        let mlx = package("kokoro_82m_mlx");
        let main = main_path_in(&root, mlx).with_extension("part");
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        std::fs::File::create(&main).unwrap().set_len(1000).unwrap();
        let config = companion_path_in(&root, mlx, &mlx.companions[0]);
        std::fs::File::create(&config).unwrap().set_len(7).unwrap();
        assert_eq!(installed_bytes_in(&root, mlx), 1007);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_partial_package_can_be_found_and_removed_whole() {
        let root = scratch("partiel-kokoro");
        let mlx = package("kokoro_82m_mlx");
        assert!(!has_files_in(&root, mlx));
        // Le gros fichier est là, la configuration et les voix non, un .part traîne.
        sparse(&root, mlx, false);
        let part = companion_path_in(&root, mlx, &mlx.companions[1]).with_extension("part");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, b"xx").unwrap();
        assert!(!is_installed_in(&root, mlx));
        assert!(has_files_in(&root, mlx));
        remove_in(&root, mlx).unwrap();
        assert!(!root.join("kokoro").exists(), "plus rien, dossier de famille retiré");
        // Seul un .part : il part aussi, sans fichier principal.
        std::fs::create_dir_all(root.join("kokoro")).unwrap();
        std::fs::write(main_path_in(&root, mlx).with_extension("part"), b"x").unwrap();
        assert!(has_files_in(&root, mlx));
        remove_in(&root, mlx).unwrap();
        assert!(!has_files_in(&root, mlx));
        assert!(!root.join("kokoro").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_a_package_with_nested_companions_removes_its_family_folder() {
        for id in ["qwen3_tts_06b_base_8bit", "magpie_tts_357m_8bit"] {
            let root = scratch(&format!("imbrique-{id}"));
            let pkg = package(id);
            sparse(&root, pkg, true);
            assert!(pkg.companions.iter().any(|c| c.dest.contains('/')), "{id} a des compagnons en sous-dossier");
            remove_in(&root, pkg).unwrap();
            assert!(!root.join(pkg.family).exists(), "{id} : dossier de famille retiré");
            std::fs::remove_dir_all(&root).ok();
        }
    }

    #[test]
    fn removing_a_partial_package_keeps_what_an_installed_one_shares() {
        let root = scratch("partiel-partage");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, int8, true);
        // Le paquet MLX n'a que quelques voix, sans son fichier principal.
        let voice = companion_path_in(&root, mlx, &mlx.companions[1]);
        assert!(voice.exists());
        remove_in(&root, mlx).unwrap();
        assert!(is_installed_in(&root, int8), "l'int8 garde voix et configuration");
        std::fs::remove_dir_all(&root).ok();
    }

    fn files_for(urls: &[(String, &Path, u64)]) -> Vec<(String, PathBuf, u64)> {
        urls.iter().map(|(u, p, b)| (u.clone(), p.to_path_buf(), *b)).collect()
    }

    fn wait(download: &Download) -> Progress {
        for _ in 0..500 {
            if download.is_finished() {
                return download.progress();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("téléchargement jamais terminé");
    }

    #[test]
    fn a_download_skips_files_already_complete_and_reports_the_package() {
        let root = scratch("telechargement");
        let (one, two) = (root.join("f").join("un.bin"), root.join("f").join("voices").join("deux.bin"));
        std::fs::create_dir_all(one.parent().unwrap()).unwrap();
        std::fs::write(&one, b"abc").unwrap();
        // Le premier est déjà là : son adresse ne répond pas et n'est pas appelée.
        let files = files_for(&[("http://127.0.0.1:1/jamais".into(), &one, 3), (serve_once(b"defgh"), &two, 5)]);
        let download = Download::start_in(&CATALOG[0], files, None).unwrap();
        assert_eq!(wait(&download), Progress::Done);
        assert_eq!(std::fs::read(&two).unwrap(), b"defgh");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_download_stops_at_the_first_failing_file() {
        let root = scratch("telechargement-echec");
        let (one, two, three) = (root.join("f/un.bin"), root.join("f/deux.bin"), root.join("f/trois.bin"));
        // Le deuxième fichier arrive trop court : le troisième n'est pas tenté.
        let files = files_for(&[
            (serve_once(b"abc"), &one, 3),
            (serve_once(b"de"), &two, 5),
            ("http://127.0.0.1:1/jamais".into(), &three, 1),
        ]);
        let download = Download::start_in(&CATALOG[0], files, None).unwrap();
        assert!(matches!(wait(&download), Progress::Failed(_)));
        assert!(one.exists() && !two.exists() && !three.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    /// Un serveur qui annonce `total` octets, en envoie `first`, le dit sur
    /// `sent`, puis attend `release` avant d'envoyer le reste.
    fn serve_in_two_parts(
        first: &'static [u8],
        rest: &'static [u8],
        sent: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/m.bin", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                first.len() + rest.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(first).unwrap();
            stream.flush().unwrap();
            let _ = sent.send(());
            let _ = release.recv();
            let _ = stream.write_all(rest);
        });
        url
    }

    #[test]
    fn a_download_cancelled_mid_package_is_cancelled_and_keeps_its_part() {
        let root = scratch("telechargement-annule");
        let (one, two, three) = (root.join("f/un.bin"), root.join("f/deux.bin"), root.join("f/trois.bin"));
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let files = files_for(&[
            (serve_once(b"abc"), &one, 3),
            (serve_in_two_parts(b"d", b"efgh", sent_tx, release_rx), &two, 5),
            ("http://127.0.0.1:1/jamais".into(), &three, 1),
        ]);
        let download = Download::start_in(&CATALOG[0], files, None).unwrap();
        sent_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let started = std::time::Instant::now();
        loop {
            match download.progress() {
                Progress::Running { received, .. } if received >= 4 => break,
                other => assert!(started.elapsed().as_secs() < 10, "pas de progression : {other:?}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        download.cancel();
        release_tx.send(()).unwrap();
        assert_eq!(wait(&download), Progress::Cancelled);
        assert_eq!(std::fs::read(&one).unwrap(), b"abc");
        assert!(!two.exists(), "le fichier annulé n'est pas installé");
        assert!(two.with_extension("part").exists(), "le .part reste pour la reprise");
        assert!(!three.exists() && !three.with_extension("part").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    /// Avec le dossier des modèles, annuler ne laisse rien : ni `.part`, ni
    /// dossier de famille. Un nouveau téléchargement repart de zéro.
    #[test]
    fn a_cancelled_download_leaves_nothing_behind() {
        let root = scratch("annulation-complete");
        let package = package("phonon_2");
        let main = main_path_in(&root, package);
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let files = files_for(&[(serve_in_two_parts(b"d", b"efgh", sent_tx, release_rx), &main, 5)]);
        let download = Download::start_in(package, files, Some(root.clone())).unwrap();
        sent_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let started = std::time::Instant::now();
        while !main.with_extension("part").exists() {
            assert!(started.elapsed().as_secs() < 10, "le .part n'arrive pas");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        download.cancel();
        release_tx.send(()).unwrap();
        assert_eq!(wait(&download), Progress::Cancelled);
        assert!(!main.exists() && !main.with_extension("part").exists(), "rien ne reste");
        assert!(!root.join(package.family).exists(), "le dossier de famille part aussi");
        assert!(!has_files_in(&root, package), "plus rien à supprimer");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn what_a_sibling_shares_is_not_this_packages_to_remove() {
        let root = scratch("partage-presence");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, int8, true);
        assert!(!has_files_in(&root, mlx), "rien qui soit au seul paquet MLX");
        assert!(has_files_in(&root, int8));
        // Le MLX arrive à moitié : son fichier principal le rend visible.
        std::fs::write(main_path_in(&root, mlx).with_extension("part"), b"x").unwrap();
        assert!(has_files_in(&root, mlx));
        // Un frère qui télécharge protège ses compagnons communs.
        remove_in(&root, int8).unwrap();
        assert!(companion_path_in(&root, int8, &int8.companions[0]).exists(), "le MLX en cours les attend");
        assert!(!main_path_in(&root, int8).exists());
        remove_in(&root, mlx).unwrap();
        assert!(!root.join("kokoro").exists(), "plus personne : tout est retiré");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_the_mlx_only_files_leaves_the_shared_ones() {
        let root = scratch("seuls-au-mlx");
        let (mlx, int8) = (package("kokoro_82m_mlx"), package("kokoro_82m_int8"));
        sparse(&root, mlx, true);
        std::fs::write(main_path_in(&root, int8), b"x").unwrap(); // l'autre paquet est là, voix partagées
        remove_in(&root, mlx).unwrap();
        assert!(!main_path_in(&root, mlx).exists());
        assert!(companion_path_in(&root, mlx, &mlx.companions[0]).exists());
        std::fs::remove_dir_all(&root).ok();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = crate::paths::test_scratch(&format!("models-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_download_of_the_wrong_size_is_refused_and_forgotten() {
        // Une reprise sur un blob qui a changé donne un fichier de la bonne
        // apparence et du mauvais contenu ; un disque plein, un fichier court.
        // La taille annoncée fait foi, et un `.part` faux ne sert pas de base.
        let dir = scratch("tronque");
        let destination = dir.join("m.gguf");
        let partial = dir.join("m.part");
        std::fs::write(&partial, [0u8; 7]).unwrap();
        assert!(settle(&partial, &destination, 10).is_err());
        assert!(!partial.exists(), "le .part faux ne doit pas servir de reprise");
        assert!(!destination.exists());
        std::fs::write(&partial, [0u8; 10]).unwrap();
        settle(&partial, &destination, 10).unwrap();
        assert!(destination.is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_partial_file_already_complete_needs_nothing_from_the_network() {
        // Un `.part` de la taille annoncée n'a plus rien à recevoir : le
        // redemander au serveur rend 416, et l'installation échouait là.
        let dir = scratch("complet");
        let partial = dir.join("m.part");
        std::fs::write(&partial, [0u8; 10]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::Complete);
        std::fs::write(&partial, [0u8; 4]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::At(4));
        // Plus long que prévu : il ne vient pas du même fichier.
        std::fs::write(&partial, [0u8; 12]).unwrap();
        assert_eq!(resume_from(&partial, 10), Resume::FromScratch);
        assert!(!partial.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_active_path_is_recognised_through_tilde_and_links() {
        let root = crate::paths::test_scratch("active");
        let _ = std::fs::remove_dir_all(&root);
        let real = root.join("home/models");
        std::fs::create_dir_all(&real).unwrap();
        let model = real.join("m.gguf");
        std::fs::write(&model, b"x").unwrap();
        let home = root.join("home");

        assert!(is_active_path_with("~/models/m.gguf", &model, Some(&home)), "forme ~");
        assert!(!is_active_path_with("~/models/autre.gguf", &model, Some(&home)));
        assert!(!is_active_path_with("", &model, Some(&home)));

        #[cfg(unix)]
        {
            let link = root.join("lien");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let via_link = link.join("m.gguf");
            assert!(is_active_path_with(&via_link.to_string_lossy(), &model, Some(&home)), "dossier lié");
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn kokoro_names_its_voices_by_language() {
        assert!(voices_follow_language("kokoro"));
        assert!(!voices_follow_language("supertonic"));
        // Deux françaises, vingt-huit anglaises : toutes les voix du paquet.
        let ids = |language: &str| -> Vec<&str> {
            voices_for_family("kokoro").iter().filter(|v| v.language == language).map(|v| v.id).collect()
        };
        assert_eq!(ids("fr"), ["p-fenrir60", "ff_siwis"]);
        assert_eq!(&ids("en")[..2], ["af_bella", "af_heart"]);
        assert_eq!(ids("en").len(), 28);
        assert_eq!(voices_for_family("kokoro").len(), 30);
        assert_eq!(default_voice_for_language("kokoro", "fr"), "p-fenrir60");
        assert_eq!(default_voice_for_language("kokoro", "en-US"), "af_bella");
        assert!(voices_for_family("kokoro").iter().all(|v| v.label.get().contains('·')));
    }

    #[test]
    fn an_unknown_voice_gives_the_default_of_its_language() {
        assert_eq!(voice_for_language("kokoro", "en", Some("am_fenrir")), "am_fenrir");
        // Une voix inconnue retombe sur la recommandée de la langue.
        assert_eq!(voice_for_language("kokoro", "fr", Some("voix-disparue")), "p-fenrir60");
        assert_eq!(voice_for_language("kokoro", "en", Some("bf_emma")), "bf_emma");
        assert_eq!(voice_for_language("kokoro", "fr", Some("af_bella")), "af_bella", "voix d'un autre accent");
        assert_eq!(voice_for_language("kokoro", "en", Some("F1")), "af_bella");
        assert_eq!(voice_for_language("kokoro", "en", None), "af_bella");
    }

    #[test]
    fn the_recommended_kokoro_is_chosen_when_none_is_installed() {
        let mlx = package("kokoro_82m_mlx");
        let int8 = package("kokoro_82m_int8");
        assert_eq!(default_voice_to_install(&[mlx], |_| false), Some(mlx));
        assert_eq!(default_voice_to_install(&[int8], |_| false), Some(int8));
    }

    #[test]
    fn an_installed_kokoro_needs_no_installation() {
        let mlx = package("kokoro_82m_mlx");
        assert_eq!(default_voice_to_install(&[mlx], |_| true), None);
    }

    #[test]
    fn a_build_without_kokoro_installs_nothing() {
        assert_eq!(default_voice_to_install(&[], |_| false), None);
    }

    /// Appelée à chaque lancement et à chaque « Réessayer » : un
    /// téléchargement qui court n'est jamais doublé (rien ne part sur le
    /// réseau ici).
    #[test]
    fn a_running_download_is_never_doubled() {
        let running = Download::fake(&CATALOG[0], false);
        assert!(ensure_default_voice(true, Some(&running)).unwrap().is_none());
    }

    /// Un terminal construit sans `kokoro-onnx` n'a aucun Kokoro à installer.
    #[cfg(not(feature = "kokoro-onnx"))]
    #[test]
    fn the_terminal_without_onnx_has_nothing_to_install() {
        assert!(ensure_default_voice(false, None).unwrap().is_none());
    }
}

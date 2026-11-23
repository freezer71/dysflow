//! Le modèle Qwen3-ASR : chargement d'un dossier MLX (comme
//! `Qwen3ASRModel.fromModelDirectory` : `config.json`, le vocabulaire BPE,
//! les `*.safetensors` passés par `sanitize`, la quantification de
//! `config.json` sur les couches qui ont leurs `scales`, hors de l'encodeur
//! audio ; chaque tenseur à son type du fichier), puis la lecture
//! (`generate` : tranches aux silences, mel, encodeur, invite où les
//! `<|audio_pad|>` prennent la sortie de l'encodeur, préremplissage par
//! paquets, décodage glouton pas à pas).

use anyhow::{bail, Result};
use mlx_rs::ops::indexing::{argmax_axis, TryIndexOp};
use mlx_rs::{ops, Array};
use std::cell::Cell;
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;

use super::audio::{preprocess, split_into_chunks, SAMPLE_RATE};
use super::config::{Config, CONFIG_FILE, NAME};
use super::decoder::Decoder;
use super::encoder::Encoder;
use crate::compute::mlx::tokenizer::bpe::BpeTokenizer;
use crate::stt::mlx::nn::KvCache;
use crate::stt::mlx::weights::Weights;

/// Les jetons gardés au plus (`defaultGenerationParameters.maxTokens`), pour
/// toute la lecture : partagés entre les tranches.
pub const MAX_TOKENS: usize = 8192;
/// Une tranche au plus, en secondes (`chunkDuration` : 20 min).
pub const CHUNK_DURATION: f32 = 1200.0;
/// Une tranche au moins, en secondes (`minChunkDuration`) : plus court, elle
/// est complétée de zéros.
pub const MIN_CHUNK_DURATION: f32 = 1.0;
/// Les fins de texte (`<|im_end|>`, `<|endoftext|>`), écrites en dur dans le
/// Swift.
pub const EOS: [u32; 2] = [151_645, 151_643];
/// Les positions préremplies par passage (`prefillStepSize`).
const PREFILL_STEP: i32 = 2048;
/// Le cache de MLX est vidé tous les 256 jetons.
const CLEAR_EVERY: usize = 256;
/// Le garde-fou des répétitions : sur les 24 derniers jetons, 3 ids
/// différents au plus arrêtent la tranche (pénalité de répétition à 1).
const REPEAT_WINDOW: usize = 24;
const REPEAT_DISTINCT: usize = 3;

/// `qwen3ASRLanguageAliases`.
const ALIASES: [(&str, &str); 23] = [
    ("zh", "Chinese"),
    ("chinese", "Chinese"),
    ("mandarin", "Chinese"),
    ("yue", "Cantonese"),
    ("cantonese", "Cantonese"),
    ("en", "English"),
    ("english", "English"),
    ("de", "German"),
    ("german", "German"),
    ("es", "Spanish"),
    ("spanish", "Spanish"),
    ("fr", "French"),
    ("french", "French"),
    ("it", "Italian"),
    ("italian", "Italian"),
    ("pt", "Portuguese"),
    ("portuguese", "Portuguese"),
    ("ru", "Russian"),
    ("russian", "Russian"),
    ("ko", "Korean"),
    ("korean", "Korean"),
    ("ja", "Japanese"),
    ("japanese", "Japanese"),
];

const AUDIO_START: &str = "<|audio_start|>";
const ASR_TEXT: &str = "<asr_text>";
const LANGUAGE_PREFIX: &str = "language ";

/// Une tranche lue : son invite et ses jetons gardés (sans la fin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkTrace {
    pub prompt: Vec<u32>,
    pub tokens: Vec<u32>,
}

/// Ce qu'une lecture a rendu : chaque tranche, et le texte (`generate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub chunks: Vec<ChunkTrace>,
    pub text: String,
}

pub struct Qwen3Asr {
    pub config: Config,
    pub tokenizer: BpeTokenizer,
    pub encoder: Encoder,
    pub decoder: Decoder,
}

impl Qwen3Asr {
    /// `fromModelDirectory` : `config.json` et le vocabulaire d'abord (un
    /// fichier absent ou illisible échoue avant les poids), puis les poids,
    /// chaque clé prise avec sa forme ; une clé absente ou en trop est une
    /// erreur (`verify: .all`).
    pub fn load(dir: &Path) -> Result<Self> {
        if !dir.join(CONFIG_FILE).is_file() {
            bail!(crate::tr!(
                en = "{NAME}: {CONFIG_FILE} is missing from {}",
                fr = "{NAME} : {CONFIG_FILE} absent de {}",
                dir.display()
            ));
        }
        let config = Config::load(dir)?;
        // Les lignes de l'encodeur prennent la place de plongements du
        // décodeur : même largeur (les défauts du Swift, eux, diffèrent).
        if config.audio.output_dim != config.text.hidden_size {
            bail!(crate::tr!(
                en = "{NAME}: audio_config.output_dim = {}, but text_config.hidden_size = {}",
                fr = "{NAME} : audio_config.output_dim = {}, mais text_config.hidden_size = {}",
                config.audio.output_dim,
                config.text.hidden_size,
            ));
        }
        let tokenizer = BpeTokenizer::from_dir(dir)?;
        let tied = config.text.tie_word_embeddings;
        let thinker = Cell::new(false);
        let mut w = Weights::load_as_stored(dir, NAME, config.quantization, |key| {
            sanitize_key(key, tied, &thinker)
        })?;
        if thinker.get() {
            transpose_convolutions(&mut w)?;
        }
        let encoder = Encoder::load(&mut w, &config)?;
        let decoder = Decoder::load(&mut w, &config.text)?;
        w.finish()?;
        Ok(Self {
            config,
            tokenizer,
            encoder,
            decoder,
        })
    }

    /// `normalizeLanguageName` : une langue du modèle (`support_languages`,
    /// sans égard à la casse), un alias (« fr » → « French »), sinon telle
    /// que donnée ; vide : aucune.
    pub fn normalize_language(&self, language: Option<&str>) -> Option<String> {
        normalize_language(&self.config.support_languages, language)
    }

    /// `buildPrompt(numAudioTokens:context:language:)`, sans contexte : le
    /// gabarit encodé sans ses `<|audio_pad|>`, puis leur id inséré tel quel
    /// après le dernier `<|audio_start|>`.
    pub fn prompt(&self, audio_tokens: usize, language: Option<&str>) -> Result<Vec<u32>> {
        prompt_ids(&self.tokenizer, &self.config, audio_tokens, language)
    }

    /// `preprocessAudio` : le mel `[1, bandes, trames]` et les jetons audio.
    pub fn features(&self, samples: &[f32]) -> Result<(Array, usize)> {
        let (features, tokens) = preprocess(samples, self.config.audio.num_mel_bins as usize)?;
        Ok((features, usize::try_from(tokens).unwrap_or(0)))
    }

    /// `getAudioFeatures`, évalué : `[positions, output_dim]`.
    pub fn encode(&self, features: &Array) -> Result<Array> {
        let encoded = self.encoder.forward(features)?;
        encoded.eval()?;
        Ok(encoded)
    }

    /// `generate` : l'audio coupé en tranches de 20 min au plus (une seconde
    /// au moins), chacune lue seule, au plus `max_tokens` jetons pour le
    /// tout ; les textes joints par une espace. La langue est imposée si
    /// donnée (l'invite finit par « language French<asr_text> »), sinon le
    /// modèle l'écrit avant `<asr_text>` et elle est retirée du texte.
    /// `stop` est consulté après chaque encodeur et chaque jeton.
    pub fn generate(
        &self,
        samples: &[f32],
        language: Option<&str>,
        max_tokens: usize,
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<Transcript> {
        let forced = self.normalize_language(language);
        let chunks = split_into_chunks(samples, SAMPLE_RATE, CHUNK_DURATION, MIN_CHUNK_DURATION);
        let mut remaining = max_tokens;
        let mut texts = Vec::new();
        let mut traces = Vec::new();
        for chunk in chunks {
            if remaining == 0 {
                break;
            }
            let trace = self.generate_chunk(&chunk.samples, forced.as_deref(), remaining, stop)?;
            let text = self.chunk_text(&trace.tokens, forced.as_deref());
            remaining = remaining.saturating_sub(trace.tokens.len());
            texts.push(text);
            traces.push(trace);
            crate::compute::mlx::clear_cache()?;
        }
        Ok(Transcript {
            chunks: traces,
            text: texts.join(" ").trim().to_string(),
        })
    }

    /// Le texte d'une tranche (`parseGeneratedChunk`).
    fn chunk_text(&self, tokens: &[u32], forced: Option<&str>) -> String {
        let decoded = self.tokenizer.decode(tokens);
        parse_generated(&self.config.support_languages, &decoded, forced)
    }

    /// `generateSingleChunk`, température 0, pénalité de répétition 1.
    pub fn generate_chunk(
        &self,
        samples: &[f32],
        language: Option<&str>,
        max_tokens: usize,
        stop: &dyn Fn() -> Result<()>,
    ) -> Result<ChunkTrace> {
        let (features, audio_tokens) = self.features(samples)?;
        let prompt = self.prompt(audio_tokens, language)?;
        let vocab = self.config.text.vocab_size as u32;
        if let Some(id) = prompt.iter().find(|&&id| id >= vocab) {
            bail!(crate::tr!(
                en = "{NAME}: prompt token {id} outside the vocabulary of {vocab}",
                fr = "{NAME} : jeton d'invite {id} hors du vocabulaire de {vocab}",
            ));
        }
        let audio = self.encode(&features)?;
        stop()?;

        let ids: Vec<i32> = prompt.iter().map(|&id| id as i32).collect();
        let total = ids.len() as i32;
        let embeds = self
            .decoder
            .embed_tokens
            .forward(&Array::from_slice(&ids, &[1, total]))?;
        let embeds = self.merge_audio(&embeds, &audio.as_dtype(embeds.dtype())?, &prompt)?;

        let mut cache = self.decoder.make_cache();
        let mut processed = 0;
        while total - processed > 1 {
            let n = PREFILL_STEP.min(total - processed - 1);
            let chunk = embeds.try_index((.., processed..processed + n, ..))?;
            let logits = self.decoder.forward(&chunk, &mut cache)?;
            logits.eval()?;
            crate::compute::mlx::clear_cache()?;
            processed += n;
        }
        let last = embeds.try_index((.., processed..total, ..))?;
        let logits = self.decoder.forward(&last, &mut cache)?;
        let mut next = last_argmax(&logits)?;

        let mut generated: Vec<u32> = Vec::new();
        for index in 0..max_tokens {
            let token = next.try_item_exact::<u32>()?;
            if EOS.contains(&token) {
                break;
            }
            generated.push(token);
            stop()?;
            if repeats(&generated) || index + 1 == max_tokens {
                break;
            }
            next = self.step(token, &mut cache)?;
            if index > 0 && index % CLEAR_EVERY == 0 {
                crate::compute::mlx::clear_cache()?;
            }
        }
        Ok(ChunkTrace {
            prompt,
            tokens: generated,
        })
    }

    /// Un pas : le jeton seul, par le cache ; l'argmax de ses logits, pas
    /// encore évalué.
    fn step(&self, token: u32, cache: &mut [KvCache]) -> Result<Array> {
        let ids = Array::from_slice(&[token as i32], &[1, 1]);
        let embeds = self.decoder.embed_tokens.forward(&ids)?;
        last_argmax(&self.decoder.forward(&embeds, cache)?)
    }

    /// `mergeAudioFeatures` : les `<|audio_pad|>` (contigus) de l'invite
    /// prennent les lignes de l'encodeur, autant qu'il y en a des deux côtés ;
    /// sans l'un ou l'autre, les plongements tels quels.
    pub fn merge_audio(&self, embeds: &Array, audio: &Array, prompt: &[u32]) -> Result<Array> {
        let pad = self.config.audio_token_id as u32;
        let count = prompt.iter().filter(|&&id| id == pad).count() as i32;
        let rows = audio.shape().first().copied().unwrap_or(0);
        let Some(first) = prompt.iter().position(|&id| id == pad) else {
            return Ok(embeds.clone());
        };
        if count == 0 || rows == 0 {
            return Ok(embeds.clone());
        }
        let shape = embeds.shape().to_vec();
        let hidden = shape.last().copied().unwrap_or(0);
        let flat = embeds.reshape(&[-1, hidden])?;
        let total = flat.shape()[0];
        let (first, replaced) = (first as i32, count.min(rows));
        let end = first + replaced;
        let mut parts = Vec::new();
        if first > 0 {
            parts.push(flat.try_index(0..first)?);
        }
        parts.push(audio.try_index(0..replaced)?);
        if end < total {
            parts.push(flat.try_index(end..total)?);
        }
        Ok(ops::concatenate(&parts, 0)?.reshape(&shape)?)
    }
}

/// L'argmax des logits de la dernière position (`logits[0..., -1, 0...]`).
fn last_argmax(logits: &Array) -> Result<Array> {
    let time = logits.shape().get(1).copied().unwrap_or(1);
    let last = logits.try_index((.., time - 1, ..))?;
    Ok(argmax_axis(&last, -1, None)?)
}

/// Le garde-fou des boucles : 24 jetons au moins, et au plus 3 ids
/// différents parmi les 24 derniers.
fn repeats(tokens: &[u32]) -> bool {
    if tokens.len() < REPEAT_WINDOW {
        return false;
    }
    let tail = &tokens[tokens.len() - REPEAT_WINDOW..];
    let distinct: std::collections::BTreeSet<u32> = tail.iter().copied().collect();
    distinct.len() <= REPEAT_DISTINCT
}

/// `buildPrompt` sans le modèle : le vocabulaire et `config.json` suffisent.
pub fn prompt_ids(
    tokenizer: &BpeTokenizer,
    config: &Config,
    audio_tokens: usize,
    language: Option<&str>,
) -> Result<Vec<u32>> {
    let language = normalize_language(&config.support_languages, language);
    let text = prompt_text(language.as_deref());
    let Some(cut) = text.rfind(AUDIO_START).map(|at| at + AUDIO_START.len()) else {
        bail!(crate::tr!(
            en = "{NAME}: template without {AUDIO_START}",
            fr = "{NAME} : gabarit sans {AUDIO_START}",
        ));
    };
    let mut ids = tokenizer.encode(&text[..cut], true);
    ids.extend(std::iter::repeat_n(
        config.audio_token_id as u32,
        audio_tokens,
    ));
    ids.extend(tokenizer.encode(&text[cut..], true));
    Ok(ids)
}

/// `buildPromptText(numAudioTokens: 0, context: "", language:)`.
pub fn prompt_text(language: Option<&str>) -> String {
    let prefix = language.map_or(String::new(), |name| {
        format!("{LANGUAGE_PREFIX}{name}{ASR_TEXT}")
    });
    format!(
        "<|im_start|>system\n<|im_end|>\n<|im_start|>user\n{AUDIO_START}<|audio_end|><|im_end|>\n<|im_start|>assistant\n{prefix}"
    )
}

/// `normalizeLanguageName`, sur les langues du modèle.
pub fn normalize_language(supported: &[String], language: Option<&str>) -> Option<String> {
    let raw = language?.trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_lowercase();
    let find = |name: &str| supported.iter().find(|s| s.to_lowercase() == name).cloned();
    if let Some(name) = find(&lower) {
        return Some(name);
    }
    if let Some(&(_, alias)) = ALIASES.iter().find(|(key, _)| *key == lower) {
        if supported.is_empty() {
            return Some(alias.to_string());
        }
        if let Some(name) = find(&alias.to_lowercase()) {
            return Some(name);
        }
    }
    Some(raw.to_string())
}

/// `extractLanguage` : « language X<asr_text>texte » → (X, texte), sans
/// espaces autour ; sans ce préfixe, (rien, le texte).
pub fn extract_language(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim();
    // `hasPrefix` du Swift compare des graphèmes : « language » suivi d'une
    // marque combinante n'est pas le préfixe.
    let prefixed = trimmed
        .graphemes(true)
        .take(LANGUAGE_PREFIX.len())
        .eq(LANGUAGE_PREFIX.graphemes(true));
    let (true, Some(marker)) = (prefixed, trimmed.find(ASR_TEXT)) else {
        return (None, trimmed.to_string());
    };
    let start = LANGUAGE_PREFIX.len().min(marker);
    let detected = trimmed[start..marker].trim();
    let transcript = trimmed[marker + ASR_TEXT.len()..].trim();
    (
        (!detected.is_empty()).then(|| detected.to_string()),
        transcript.to_string(),
    )
}

/// `parseGeneratedChunk` : le texte d'une tranche. Langue imposée : le texte
/// décodé tel quel, sans espaces autour ; sinon, sans la langue écrite par
/// le modèle.
pub fn parse_generated(supported: &[String], decoded: &str, forced: Option<&str>) -> String {
    let trimmed = decoded.trim();
    if normalize_language(supported, forced).is_some() {
        return trimmed.to_string();
    }
    let (language, text) = extract_language(decoded);
    if normalize_language(supported, language.as_deref()).is_some() {
        return text;
    }
    trimmed.to_string()
}

/// `sanitize` : la clé du graphe, ou `None` pour une clé ignorée. Le
/// préfixe `thinker.` (Hugging Face) est retiré, et noté dans `thinker` ;
/// `lm_head.weight` est écarté si les plongements sont liés.
fn sanitize_key(key: &str, tied: bool, thinker: &Cell<bool>) -> Option<String> {
    let key = match key.strip_prefix("thinker.") {
        Some(rest) => {
            thinker.set(true);
            rest
        }
        None => key,
    };
    if tied && key == "lm_head.weight" {
        return None;
    }
    Some(key.to_string())
}

/// Les poids de PyTorch (`thinker.`) : chaque poids 4-D de `conv2d`, de
/// `[sorties, entrées, h, l]` à `[sorties, h, l, entrées]`.
fn transpose_convolutions(w: &mut Weights) -> Result<()> {
    for name in ["conv2d1", "conv2d2", "conv2d3"] {
        let key = format!("audio_tower.{name}.weight");
        if let Some((array, file)) = w.take_any(&key) {
            let array = if array.ndim() == 4 {
                let t = array.transpose_axes(&[0, 2, 3, 1])?;
                t.eval()?;
                t
            } else {
                array
            };
            w.insert(&key, array, &file);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supported() -> Vec<String> {
        ["Chinese", "English", "French"].map(String::from).to_vec()
    }

    #[test]
    fn the_language_is_normalized_like_the_swift() {
        let s = supported();
        for (raw, expected) in [
            (Some("fr"), Some("French")),
            (Some(" FR "), Some("French")),
            (Some("french"), Some("French")),
            (Some("en"), Some("English")),
            (Some("mandarin"), Some("Chinese")),
            (Some("de"), Some("de")),
            (Some("fr-FR"), Some("fr-FR")),
            (Some(""), None),
            (Some("  "), None),
            (None, None),
        ] {
            assert_eq!(normalize_language(&s, raw).as_deref(), expected, "{raw:?}");
        }
        assert_eq!(
            normalize_language(&[], Some("de")).as_deref(),
            Some("German")
        );
        assert_eq!(
            normalize_language(&[], Some("Klingon")).as_deref(),
            Some("Klingon")
        );
    }

    /// Le cas propre de la tâche : l'invite finit par « language
    /// French<asr_text> ».
    #[test]
    fn the_prompt_ends_with_the_forced_language() {
        assert!(prompt_text(Some("French"))
            .ends_with("<|im_start|>assistant\nlanguage French<asr_text>"));
        assert!(prompt_text(None).ends_with("<|im_start|>assistant\n"));
        assert!(prompt_text(None).starts_with(
            "<|im_start|>system\n<|im_end|>\n<|im_start|>user\n<|audio_start|><|audio_end|>"
        ));
    }

    #[test]
    fn the_language_written_by_the_model_is_taken_out() {
        assert_eq!(
            extract_language("  language French<asr_text> Bonjour. "),
            (Some("French".into()), "Bonjour.".into())
        );
        assert_eq!(extract_language("language <asr_text>x"), (None, "x".into()));
        assert_eq!(extract_language("Bonjour."), (None, "Bonjour.".into()));
        assert_eq!(
            extract_language("language French sans marque"),
            (None, "language French sans marque".into())
        );
        assert_eq!(
            extract_language("language\u{301} French<asr_text>x"),
            (None, "language\u{301} French<asr_text>x".into())
        );
        let s = supported();
        assert_eq!(
            parse_generated(&s, " language French<asr_text>Oui ", None),
            "Oui"
        );
        assert_eq!(
            parse_generated(&s, " language French<asr_text>Oui ", Some("fr")),
            "language French<asr_text>Oui"
        );
        assert_eq!(parse_generated(&s, " Oui ", None), "Oui");
        assert_eq!(parse_generated(&s, "  ", None), "");
    }

    #[test]
    fn the_repetition_guard_needs_24_tokens_and_3_ids_at_most() {
        let mut tokens: Vec<u32> = (0..23).map(|i| i % 2).collect();
        assert!(!repeats(&tokens));
        tokens.push(0);
        assert!(repeats(&tokens));
        let varied: Vec<u32> = (0..24).map(|i| i % 4).collect();
        assert!(!repeats(&varied));
    }

    #[test]
    fn keys_are_sanitized_like_the_swift() {
        let thinker = Cell::new(false);
        assert_eq!(
            sanitize_key("model.norm.weight", true, &thinker).as_deref(),
            Some("model.norm.weight")
        );
        assert!(!thinker.get());
        assert_eq!(sanitize_key("lm_head.weight", true, &thinker), None);
        assert_eq!(
            sanitize_key("lm_head.weight", false, &thinker).as_deref(),
            Some("lm_head.weight")
        );
        assert_eq!(
            sanitize_key("lm_head.scales", true, &thinker).as_deref(),
            Some("lm_head.scales")
        );
        assert_eq!(
            sanitize_key("thinker.audio_tower.conv2d1.weight", true, &thinker).as_deref(),
            Some("audio_tower.conv2d1.weight")
        );
        assert!(thinker.get());
        assert_eq!(sanitize_key("thinker.lm_head.weight", true, &thinker), None);
    }
}

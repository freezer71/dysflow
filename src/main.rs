//! `reader` — lecteur audio en terminal pour du texte pipé ou un fichier.

use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use rs_reader::config::Config;
use rs_reader::doc::DocumentBuilder;
use rs_reader::input::InputEvent;
use rs_reader::tui::app::App;
use rs_reader::{doc, input, tts, tui};

#[derive(Parser, Debug)]
#[command(
    name = "reader",
    version,
    about = "Lit à voix haute du markdown pipé ou un fichier, avec contrôles de lecture complets."
)]
struct Args {
    /// Fichier à lire. En son absence, le contenu est lu sur l'entrée standard.
    file: Option<PathBuf>,

    /// Voix, préréglage, ou chemin d'un WAV de référence.
    #[arg(short, long)]
    voice: Option<String>,

    /// Débit de lecture (0,5 à 3,0).
    #[arg(short, long)]
    speed: Option<f32>,

    /// Hauteur de voix (-1,0 à 1,0).
    #[arg(long)]
    pitch: Option<f32>,

    /// Intonation demandée au modèle.
    #[arg(long)]
    emotion: Option<String>,

    /// Consigne de style en langage naturel.
    #[arg(long)]
    instruction: Option<String>,

    /// Langue du texte.
    #[arg(long)]
    language: Option<String>,

    /// Moteur de synthèse : auto, audio_cpp ou system.
    #[arg(long)]
    engine: Option<String>,

    /// Chemin du modèle audio.cpp (répertoire ou GGUF).
    #[arg(long)]
    model: Option<String>,

    /// Famille audio.cpp, par exemple supertonic.
    #[arg(long)]
    family: Option<String>,

    /// Backend de calcul : cpu, metal, cuda, vulkan.
    #[arg(long)]
    compute: Option<String>,

    /// Sessions de synthèse simultanées.
    #[arg(long)]
    sessions: Option<u32>,

    /// Phrases synthétisées en avance.
    #[arg(long)]
    prefetch: Option<u32>,

    /// Traitement des blocs de code : announce, skip ou read.
    #[arg(long)]
    code_blocks: Option<String>,

    /// Ne pas démarrer la lecture automatiquement.
    #[arg(long)]
    no_autoplay: bool,

    /// Option libre transmise au modèle, répétable : --set cle=valeur
    #[arg(long = "set", value_name = "CLE=VALEUR")]
    set: Vec<String>,

    /// Écrit la configuration résultante puis quitte.
    #[arg(long)]
    write_config: bool,

    /// Affiche la configuration résultante puis quitte.
    #[arg(long)]
    show_config: bool,

    /// Écrit le markdown extrait du fichier puis quitte.
    #[arg(long)]
    to_markdown: bool,

    /// Lit le presse-papiers au lieu d'un fichier.
    #[arg(long)]
    clip: bool,

    /// Refait l'extraction sans consulter le cache.
    #[arg(long)]
    no_cache: bool,

    /// Ouvre directement l'écran de configuration.
    #[arg(long)]
    config: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (mut config, config_notice) = Config::load();
    let mut notices: Vec<String> = config_notice.into_iter().collect();

    apply_overrides(&mut config, &args, &mut notices)?;

    if args.show_config {
        print!("{}", toml::to_string_pretty(&config)?);
        return Ok(());
    }
    if args.write_config {
        let path = config.save()?;
        println!("Configuration écrite dans {}", path.display());
        return Ok(());
    }

    // Rien à dire à voix haute : écrire du markdown sur la sortie standard ne
    // demande aucun modèle, et n'a pas à en payer le chargement.
    if args.to_markdown {
        if let Some(text) = extracted(&args)? {
            print!("{text}");
            return Ok(());
        }
    }

    // Un modèle déjà installé devient le défaut, sans rien demander.
    adopt_installed_model(&mut config, &mut notices);

    // Une configuration écrite pour un autre modèle peut nommer une voix que
    // celui-ci ne connaît pas ; le moteur la rejetterait sans rien expliquer.
    if config.align_voice_with_family() {
        notices.push(format!(
            "voix inconnue de {} — repli sur sa voix par défaut",
            config.engine.family
        ));
    }

    // La redirection couvre le chargement du modèle, qui est déjà bavard, et
    // tout ce qui suit : le chargement se poursuit en fond pendant la lecture
    // du document, et ses messages s'écriraient sinon par-dessus.
    let quiet = rs_reader::logging::redirect_stderr();

    // Le modèle part en premier, et le document se lit pendant qu'il charge.
    // Mesuré : trois secondes de chargement, une de plus pour ouvrir la
    // première session — les passer à extraire et analyser plutôt qu'à
    // attendre avance d'autant le premier son. Un PDF reconnu à l'OCR y gagne
    // des secondes entières ; un markdown, le temps de son analyse.
    let (engine, engine_notice) =
        tts::build_engine(config.engine_preference(), &config.engine_settings());
    notices.extend(engine_notice);

    let (builder, stream, source) = match read_input(&args, &config) {
        Ok(read) => read,
        // Ce qui a échoué doit s'afficher : la redirection tombe avant.
        Err(err) => {
            drop(quiet);
            return Err(err);
        }
    };

    // Sans document ni flux, on démarre sur l'écran de configuration.
    let open_config = args.config || (args.file.is_none() && !args.clip && stream.is_none());
    if (args.file.is_some() || args.clip) && builder.sentence_count() == 0 {
        drop(quiet);
        anyhow::bail!("aucun contenu lisible");
    }

    let mut app = App::new(
        builder,
        stream,
        engine,
        config,
        source,
        notices,
        rs_reader::audio::Player::new(),
    );
    // Un fichier a une identité stable : sa lecture reprend où elle en était.
    // Un flux et un collage n'en ont pas — la même position leur désignerait
    // un autre texte.
    if args.file.is_some() {
        app.resume_where_left();
    }
    if open_config {
        app.open_config();
    }
    let outcome = tui::run(app);
    // La garde tombe ici : une erreur remontée par `run` retrouve une sortie
    // d'erreur normale avant d'être affichée.
    drop(quiet);
    outcome
}

/// Le texte d'une source qui en porte un, pour `--to-markdown`.
///
/// Rend `None` pour un flux : il n'y a rien à extraire d'une entrée qui
/// n'existe pas encore.
fn extracted(args: &Args) -> Result<Option<String>> {
    if args.clip {
        return Ok(Some(read_clipboard()?));
    }
    match &args.file {
        Some(path) => Ok(Some(rs_reader::source::load_with_cache(
            path,
            !args.no_cache,
        )?)),
        None => Ok(None),
    }
}

/// Ce qu'il y a à lire : le document assemblé, le flux qui l'alimente encore,
/// et le nom sous lequel il se retient.
fn read_input(
    args: &Args,
    config: &Config,
) -> Result<(DocumentBuilder, Option<Receiver<InputEvent>>, String)> {
    let mut builder = DocumentBuilder::new(config.code_policy());

    if args.clip {
        let text = read_clipboard()?;
        for block in input::split_blocks(&text) {
            builder.push_block(&block);
        }
        return Ok((builder, None, "presse-papiers".to_string()));
    }

    match &args.file {
        Some(path) => {
            // Un fichier est analysé d'emblée : sommaire et durée sont
            // disponibles dès l'ouverture.
            let text = rs_reader::source::load_with_cache(path, !args.no_cache)?;
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            Ok((builder, None, name))
        }
        None if std::io::stdin().is_terminal() => {
            // Lancé sans rien : il n'y a rien à lire, mais tout à régler.
            // Ouvrir la configuration vaut mieux qu'un message d'erreur.
            Ok((builder, None, "configuration".to_string()))
        }
        None => {
            // Le tube est mis de côté et le descripteur 0 rendu au terminal,
            // faute de quoi le clavier resterait muet en mode pipe.
            let piped = input::detach_piped_stdin().context("récupération du flux d'entrée")?;
            let stream = match piped {
                Some(file) => input::spawn_reader(file),
                None => input::spawn_reader(std::io::stdin()),
            };
            // Le flux est lu au fil de l'eau : la lecture commence sans
            // attendre la fin de l'entrée.
            Ok((builder, Some(stream), "stdin".to_string()))
        }
    }
}

/// Lit le presse-papiers, ou explique pourquoi il ne peut pas l'être.
fn read_clipboard() -> Result<String> {
    #[cfg(feature = "clipboard")]
    {
        rs_reader::source::clipboard()
    }
    #[cfg(not(feature = "clipboard"))]
    {
        anyhow::bail!("--clip exige un binaire compilé avec l'option `clipboard`")
    }
}

/// Adopte un modèle installé si la configuration n'en désigne aucun.
///
/// Le lecteur ne télécharge jamais de lui-même : quand rien n'est installé, il
/// se contente de dire où aller le chercher, et lit avec la voix du système en
/// attendant.
fn adopt_installed_model(config: &mut Config, notices: &mut Vec<String>) {
    if !config.engine.model_path.trim().is_empty() {
        return;
    }
    match rs_reader::models::first_installed() {
        Some(package) => {
            if let Some(path) = rs_reader::models::install_path(package) {
                config.engine.model_path = path.to_string_lossy().into_owned();
                config.engine.family = package.family.to_string();
                if config.engine.engine == "auto" {
                    config.engine.engine = "audio_cpp".to_string();
                }
            }
        }
        None => {
            if !rs_reader::models::available().is_empty() {
                notices.push(
                    "aucun modèle installé — appuyez sur `s` pour en installer un".to_string(),
                );
            }
        }
    }
}

fn apply_overrides(config: &mut Config, args: &Args, notices: &mut Vec<String>) -> Result<()> {
    if let Some(v) = &args.voice {
        config.voice.voice = v.clone();
    }
    if let Some(v) = args.speed {
        config.voice.speed = v;
    }
    if let Some(v) = args.pitch {
        config.voice.pitch = v;
    }
    if let Some(v) = &args.emotion {
        config.voice.emotion = v.clone();
    }
    if let Some(v) = &args.instruction {
        config.voice.instruction = v.clone();
    }
    if let Some(v) = &args.language {
        config.voice.language = v.clone();
    }
    if let Some(v) = &args.engine {
        if tts::EnginePreference::parse(v).is_none() {
            anyhow::bail!("moteur inconnu : {v} (attendu : auto, audio_cpp ou system)");
        }
        config.engine.engine = v.clone();
    }
    if let Some(v) = &args.model {
        config.engine.model_path = v.clone();
    }
    if let Some(v) = &args.family {
        config.engine.family = v.clone();
    }
    if let Some(v) = &args.compute {
        config.engine.compute = v.clone();
    }
    if let Some(v) = args.sessions {
        config.engine.sessions = v.max(1);
    }
    if let Some(v) = args.prefetch {
        config.engine.prefetch = v.max(2);
    }
    if let Some(v) = &args.code_blocks {
        if doc::builder::CodePolicy::parse(v).is_none() {
            anyhow::bail!("valeur inconnue : {v} (attendu : announce, skip ou read)");
        }
        config.reading.code_blocks = v.clone();
    }
    if args.no_autoplay {
        config.reading.autoplay = false;
    }

    for entry in &args.set {
        match entry.split_once('=') {
            Some((key, value)) => {
                config.extra.insert(key.trim().to_string(), value.trim().to_string());
            }
            None => notices.push(format!("--set ignoré, format attendu cle=valeur : {entry}")),
        }
    }
    Ok(())
}

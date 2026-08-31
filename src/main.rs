//! `reader` — lecteur audio en terminal pour du texte pipé ou un fichier.

use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;

use rs_reader::config::Config;
use rs_reader::doc::DocumentBuilder;
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

    let mut builder = DocumentBuilder::new(config.code_policy());
    let (stream, source) = match &args.file {
        Some(path) => {
            // Un fichier est analysé d'emblée : sommaire et durée sont
            // disponibles dès l'ouverture.
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("lecture de {}", path.display()))?;
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            (None, name)
        }
        None if std::io::stdin().is_terminal() => {
            // Lancé sans rien : il n'y a rien à lire, mais tout à régler.
            // Ouvrir la configuration vaut mieux qu'un message d'erreur.
            (None, "configuration".to_string())
        }
        None => {
            // Le tube est mis de côté et le descripteur 0 rendu au terminal,
            // faute de quoi le clavier resterait muet en mode pipe.
            let piped = input::detach_piped_stdin()
                .context("récupération du flux d'entrée")?;
            let stream = match piped {
                Some(file) => input::spawn_reader(file),
                None => input::spawn_reader(std::io::stdin()),
            };
            // Le flux est lu au fil de l'eau : la lecture commence sans
            // attendre la fin de l'entrée.
            (Some(stream), "stdin".to_string())
        }
    };

    // Sans document ni flux, on démarre sur l'écran de configuration.
    let open_config = args.config || (args.file.is_none() && stream.is_none());
    if args.file.is_some() && builder.sentence_count() == 0 {
        anyhow::bail!("aucun contenu lisible dans le fichier");
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
    // s'achève avec ce bloc : une erreur remontée ensuite s'affiche normalement.
    {
        let _quiet = rs_reader::logging::redirect_stderr();

        let (engine, engine_notice) =
            tts::build_engine(config.engine_preference(), &config.engine_settings());
        notices.extend(engine_notice);

        let mut app = App::new(
            builder,
            stream,
            engine,
            config,
            source,
            notices,
            rs_reader::audio::Player::new(),
        );
        if open_config {
            app.open_config();
        }
        // La garde tombe avec ce bloc : une erreur remontée par `run` retrouve
        // donc une sortie d'erreur normale avant d'être affichée.
        tui::run(app)
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

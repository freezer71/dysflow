use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::app::App;
use crate::config::Config;
use crate::doc::DocumentBuilder;
use crate::input::{self, InputEvent};
use crate::logging::StderrRedirect;
use crate::{doc, tts};

#[derive(Parser, Debug, Default)]
#[command(
    name = "sillon",
    version,
    about = "Lit à voix haute du markdown pipé ou un fichier, avec contrôles de lecture complets."
)]
pub struct Args {
    /// Fichier à lire. En son absence, le contenu est lu sur l'entrée standard.
    pub file: Option<PathBuf>,

    /// Voix, préréglage, ou chemin d'un WAV de référence.
    #[arg(short, long)]
    pub voice: Option<String>,

    /// Débit de lecture (0,5 à 3,0).
    #[arg(short, long)]
    pub speed: Option<f32>,

    /// Hauteur de voix (-1,0 à 1,0).
    #[arg(long)]
    pub pitch: Option<f32>,

    /// Intonation demandée au modèle.
    #[arg(long)]
    pub emotion: Option<String>,

    /// Consigne de style en langage naturel.
    #[arg(long)]
    pub instruction: Option<String>,

    /// Langue du texte.
    #[arg(long)]
    pub language: Option<String>,

    /// Moteur de synthèse : auto, audio_cpp ou system.
    #[arg(long)]
    pub engine: Option<String>,

    /// Chemin du modèle audio.cpp (répertoire ou GGUF).
    #[arg(long)]
    pub model: Option<String>,

    /// Famille audio.cpp, par exemple supertonic.
    #[arg(long)]
    pub family: Option<String>,

    /// Backend de calcul : cpu, metal, cuda, vulkan.
    #[arg(long)]
    pub compute: Option<String>,

    /// Sessions de synthèse simultanées ; 0 laisse la famille décider.
    #[arg(long)]
    pub sessions: Option<u32>,

    /// Phrases synthétisées en avance.
    #[arg(long)]
    pub prefetch: Option<u32>,

    /// Traitement des blocs de code : announce, skip ou read.
    #[arg(long)]
    pub code_blocks: Option<String>,

    /// Ne pas démarrer la lecture automatiquement.
    #[arg(long)]
    pub no_autoplay: bool,

    /// Option libre transmise au modèle, répétable : --set cle=valeur
    #[arg(long = "set", value_name = "CLE=VALEUR")]
    pub set: Vec<String>,

    /// Écrit la configuration résultante puis quitte.
    #[arg(long)]
    pub write_config: bool,

    /// Affiche la configuration résultante puis quitte.
    #[arg(long)]
    pub show_config: bool,

    /// Écrit le markdown extrait du fichier puis quitte.
    #[arg(long)]
    pub to_markdown: bool,

    /// Lit le presse-papiers au lieu d'un fichier.
    #[arg(long)]
    pub clip: bool,

    /// Refait l'extraction sans consulter le cache.
    #[arg(long)]
    pub no_cache: bool,

    /// Ouvre directement l'écran de configuration.
    #[arg(long)]
    pub config: bool,
}

/// Ce que `prepare` rend : soit tout est dit, soit un lecteur attend son écran.
pub enum Launch {
    /// Une option a répondu sur la sortie standard : rien à afficher.
    Done,
    Ready(Prepared),
}

pub struct Prepared {
    pub app: App,
    /// `--config` a été demandé explicitement.
    pub config_requested: bool,
    /// Vrai quand il y a quelque chose à lire : fichier, collage ou flux.
    pub has_input: bool,
    /// Redirection de `stderr` vers le journal. À garder tant que l'écran est
    /// ouvert, à lâcher avant d'afficher une erreur.
    pub quiet: Option<StderrRedirect>,
}

pub fn prepare(args: &Args) -> Result<Launch> {
    let (mut config, config_notice) = Config::load();
    let mut notices: Vec<String> = config_notice.into_iter().collect();

    apply_overrides(&mut config, args, &mut notices)?;

    if args.show_config {
        print!("{}", toml::to_string_pretty(&config)?);
        return Ok(Launch::Done);
    }
    if args.write_config {
        let path = config.save()?;
        println!("Configuration écrite dans {}", path.display());
        return Ok(Launch::Done);
    }

    // Rien à dire à voix haute : écrire du markdown sur la sortie standard ne
    // demande aucun modèle, et n'a pas à en payer le chargement.
    if args.to_markdown {
        if let Some(text) = extracted(args)? {
            print!("{text}");
            return Ok(Launch::Done);
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
    let quiet = crate::logging::redirect_stderr();

    // Le modèle part en premier, et le document se lit pendant qu'il charge.
    // Mesuré : trois secondes de chargement, une de plus pour ouvrir la
    // première session — les passer à extraire et analyser plutôt qu'à
    // attendre avance d'autant le premier son. Un PDF reconnu à l'OCR y gagne
    // des secondes entières ; un markdown, le temps de son analyse.
    let (engine, engine_notice) =
        tts::build_engine(config.engine_preference(), &config.engine_settings());
    notices.extend(engine_notice);

    let (builder, stream, source) = match read_input(args, &config) {
        Ok(read) => read,
        // Ce qui a échoué doit s'afficher : la redirection tombe avant.
        Err(err) => {
            drop(quiet);
            return Err(err);
        }
    };

    let has_input = args.file.is_some() || args.clip || stream.is_some();
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
        crate::audio::Player::new(),
    );
    // Un fichier a une identité stable : sa lecture reprend où elle en était.
    // Un flux et un collage n'en ont pas — la même position leur désignerait
    // un autre texte.
    if let Some(path) = &args.file {
        // Positions et signets ont pu être retenus sous le nom seul, par une
        // version antérieure : ils suivent le document.
        if let Some(former) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            app.adopt_former_identity(&former);
        }
        app.resume_where_left();
    }

    Ok(Launch::Ready(Prepared {
        app,
        config_requested: args.config,
        has_input,
        quiet,
    }))
}

/// Ce qui identifie un document d'une ouverture à l'autre : son chemin entier.
///
/// Le nom seul ne suffit pas — deux `spec.pdf` dans deux dossiers se
/// partageraient positions et signets, et l'un enverrait la lecture de
/// l'autre n'importe où. Le chemin est résolu pour qu'un même fichier, atteint
/// par deux chemins, reste un seul document.
pub fn document_identity(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
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
        Some(path) => Ok(Some(crate::source::load_with_cache(
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
            let text = crate::source::load_with_cache(path, !args.no_cache)?;
            for block in input::split_blocks(&text) {
                builder.push_block(&block);
            }
            Ok((builder, None, document_identity(path)))
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
        crate::source::clipboard()
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
    let configured = config.engine.model_path.trim();
    if !configured.is_empty() {
        // Un chemin écrit par une version antérieure peut désigner un fichier
        // qui a déménagé — le répertoire de données a changé de nom. Le
        // garder ferait tomber sur la voix système alors qu'un modèle est là.
        if std::path::Path::new(&crate::config::expand_tilde(configured)).exists() {
            return;
        }
        notices.push(format!("modèle introuvable : {configured}"));
        config.engine.model_path.clear();
    }
    match crate::models::first_installed() {
        Some(package) => {
            if let Some(path) = crate::models::install_path(package) {
                config.engine.model_path = path.to_string_lossy().into_owned();
                config.engine.family = package.family.to_string();
                if config.engine.engine == "auto" {
                    config.engine.engine = "audio_cpp".to_string();
                }
            }
        }
        None => {
            if !crate::models::available().is_empty() {
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
        // Zéro est « auto » : ce que la famille demande.
        config.engine.sessions = v.min(16);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sillon-launch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn two_files_of_the_same_name_are_two_documents() {
        // Positions et signets étaient rattachés au nom seul : ouvrir
        // b/spec.pdf écrasait la place retenue dans a/spec.pdf.
        let a = scratch("a");
        let b = scratch("b");
        std::fs::write(a.join("spec.pdf"), "x").unwrap();
        std::fs::write(b.join("spec.pdf"), "y").unwrap();
        assert_ne!(document_identity(&a.join("spec.pdf")), document_identity(&b.join("spec.pdf")));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn the_same_file_has_one_identity_however_it_is_named() {
        let dir = scratch("c");
        std::fs::write(dir.join("notes.md"), "x").unwrap();
        let detour = dir.join("..").join(dir.file_name().unwrap()).join("notes.md");
        assert_eq!(document_identity(&dir.join("notes.md")), document_identity(&detour));
        std::fs::remove_dir_all(&dir).ok();
    }


    #[test]
    fn sessions_zero_on_the_command_line_means_auto() {
        let mut config = Config::default();
        config.engine.sessions = 3;
        let args = Args { sessions: Some(0), ..Args::default() };
        apply_overrides(&mut config, &args, &mut Vec::new()).unwrap();
        assert_eq!(config.engine.sessions, 0);
    }
}

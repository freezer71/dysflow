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
    name = "sillage",
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
    #[arg(long, hide = true)]
    pub pitch: Option<f32>,

    /// Langue du texte.
    #[arg(long, hide = true)]
    pub language: Option<String>,

    /// Moteur de synthèse : auto ou system.
    #[arg(long, hide = true)]
    pub engine: Option<String>,

    /// Chemin du modèle (Kokoro).
    #[arg(long, hide = true)]
    pub model: Option<String>,

    /// Famille du modèle, par exemple kokoro.
    #[arg(long, hide = true)]
    pub family: Option<String>,

    /// Sessions de synthèse simultanées ; 0 laisse la famille décider.
    #[arg(long, hide = true)]
    pub sessions: Option<u32>,

    /// Phrases synthétisées en avance.
    #[arg(long, hide = true)]
    pub prefetch: Option<u32>,

    /// Traitement des blocs de code : announce, skip ou read.
    #[arg(long, hide = true)]
    pub code_blocks: Option<String>,

    /// Ne pas démarrer la lecture automatiquement.
    #[arg(long, hide = true)]
    pub no_autoplay: bool,

    /// Écrit la configuration résultante puis quitte.
    #[arg(long, hide = true)]
    pub write_config: bool,

    /// Affiche la configuration résultante puis quitte.
    #[arg(long, hide = true)]
    pub show_config: bool,

    /// Vérifie que Kokoro se charge (ONNX Runtime, sillage-espeak et ses
    /// données) puis quitte.
    #[arg(long, hide = true)]
    pub check_kokoro: bool,

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

    /// Met sillage à jour : installe la dernière version publiée.
    #[arg(long)]
    pub update: bool,
}

/// Ce que `sillage` lancé seul dans un terminal affiche, avant de rendre la
/// main : il n'y a rien à lire, mais tout à savoir pour lire.
pub const SHORT_HELP: &str = "\
sillage lit un texte à voix haute, en le montrant mot à mot.

Quatre manières de lire :
  sillage document.md        un fichier : markdown, texte, PDF, image
  commande | sillage         ce qu'une commande écrit, au fil de l'eau
  sillage --clip             le presse-papiers
  l'app sillage (macOS)      ⌃⌥Espace lit la sélection

Réglages : sillage --config · mise à jour : sillage --update
Toutes les options : sillage --help
";

/// Vrai quand `sillage` est lancé seul dans un terminal : ni fichier, ni
/// presse-papiers, ni réglages demandés, et rien qui arrive par un tube.
pub fn nothing_to_read(args: &Args, stdin_is_terminal: bool) -> bool {
    args.file.is_none() && !args.clip && !args.config && stdin_is_terminal
}

/// Ce que `prepare` rend : soit tout est dit, soit un lecteur attend son écran.
pub enum Launch {
    /// Une option a répondu sur la sortie standard : rien à afficher.
    Done,
    Ready(Box<Prepared>),
}

pub struct Prepared {
    pub app: App,
    /// `--config` a été demandé explicitement.
    pub config_requested: bool,
    /// Redirection de `stderr` vers le journal. À garder tant que l'écran est
    /// ouvert, à lâcher avant d'afficher une erreur.
    pub quiet: Option<StderrRedirect>,
}

pub fn prepare(args: &Args) -> Result<Launch> {
    // Avant la configuration : une configuration illisible ne doit pas
    // empêcher de se mettre à jour.
    if args.update {
        crate::update::run()?;
        return Ok(Launch::Done);
    }
    let (mut config, config_notice) = Config::load();
    // Une configuration illisible n'est jamais réécrite.
    let config_notice_is_none = config_notice.is_none();
    let mut notices: Vec<String> = config_notice.into_iter().collect();

    apply_overrides(&mut config, args)?;

    if args.show_config {
        print!("{}", toml::to_string_pretty(&config)?);
        return Ok(Launch::Done);
    }
    if args.write_config {
        let path = config.save()?;
        println!("{}", crate::tr!(en = "Configuration written to {}", fr = "Configuration écrite dans {}", path.display()));
        return Ok(Launch::Done);
    }
    if args.check_kokoro {
        print!("{}", check_kokoro()?);
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

    // Lancé seul, sans rien à lire : dire comment lire plutôt qu'ouvrir un
    // écran vide. `--config` ouvre les réglages.
    if nothing_to_read(args, std::io::stdin().is_terminal()) {
        print!("{SHORT_HELP}");
        return Ok(Launch::Done);
    }

    // Le passage à Kokoro s'écrit sur la configuration du disque, jamais sur
    // celle que les options de la ligne de commande ont modifiée.
    if config_notice_is_none {
        let (on_disk, _) = Config::load();
        let available = crate::models::available();
        if let Some(migrated) =
            migrate_for_launch(&mut config, on_disk, args, &mut notices, &available, crate::models::is_installed)
        {
            let _ = migrated.save();
        }
    }

    // Un modèle déjà installé devient le défaut, sans rien demander.
    adopt_installed_model(&mut config, &mut notices);

    // Kokoro remplit en silence ses voix par langue manquantes. Seul Kokoro
    // nomme ses voix : aucune autre famille n'a de voix à réaligner.
    config.align_voice_with_family();

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

    if (args.file.is_some() || args.clip) && builder.sentence_count() == 0 {
        drop(quiet);
        anyhow::bail!(crate::tr!("nothing readable", "aucun contenu lisible"));
    }

    let player = crate::audio::Player::with_output(&config.app.output_device);
    let mut app = App::new(builder, stream, engine, config, source, notices, player);
    // Kokoro s'installe seul au premier usage ; la voix du système lit
    // en attendant.
    app.install_default_voice();
    // Un fichier a une identité stable : sa lecture reprend où elle en était.
    // Un flux et un collage n'en ont pas — la même position leur désignerait
    // un autre texte.
    if let Some(path) = &args.file {
        // Une position a pu être retenue sous le nom seul, par une version
        // antérieure : elle suit le document.
        if let Some(former) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            app.adopt_former_identity(&former);
        }
        app.resume_where_left();
    }

    Ok(Launch::Ready(Box::new(Prepared {
        app,
        config_requested: args.config,
        quiet,
    })))
}

/// `--check-kokoro` : ce que Kokoro charge au terminal, sans le modèle
/// (téléchargé à part). `install.sh` s'en sert après l'installation.
#[cfg(feature = "kokoro-onnx")]
fn check_kokoro() -> Result<String> {
    tts::kokoro::onnx::check_install().context(crate::tr!("Kokoro does not load", "Kokoro ne se charge pas"))
}

#[cfg(not(feature = "kokoro-onnx"))]
fn check_kokoro() -> Result<String> {
    anyhow::bail!(crate::tr!(
        "Kokoro missing from this binary (built without the kokoro-onnx feature)",
        "Kokoro absent de ce binaire (construit sans la feature kokoro-onnx)"
    ))
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

/// Un document prêt à recevoir `text` : la langue vient du choix retenu pour
/// lui, de la configuration, ou de la détection.
fn builder_for(config: &Config, text: &str, identity: Option<&str>) -> DocumentBuilder {
    let detected = crate::text::language::detect_markdown(text);
    let choices = crate::reader::language::LanguageChoices::load();
    let chosen = identity.and_then(|id| choices.get(id)).map(str::to_string);
    let language = crate::text::language::resolve(&config.voice.language, chosen.as_deref(), detected);
    DocumentBuilder::with_language(config.code_policy(), &language)
}

/// Ce qu'il y a à lire : le document assemblé, le flux qui l'alimente encore,
/// et le nom sous lequel il se retient.
fn read_input(
    args: &Args,
    config: &Config,
) -> Result<(DocumentBuilder, Option<Receiver<InputEvent>>, String)> {
    if args.clip {
        let text = read_clipboard()?;
        let mut builder = builder_for(config, &text, None);
        for block in crate::doc::blocks::split_blocks(&text) {
            builder.push_block(&block);
        }
        return Ok((builder, None, crate::tr!("clipboard", "presse-papiers").to_string()));
    }

    match &args.file {
        Some(path) => {
            // Un fichier est analysé d'emblée : sommaire et durée sont
            // disponibles dès l'ouverture.
            let text = crate::source::load_with_cache(path, !args.no_cache)?;
            let identity = crate::position::document_identity(path);
            let mut builder = builder_for(config, &text, Some(&identity));
            for block in crate::doc::blocks::split_blocks(&text) {
                builder.push_block(&block);
            }
            Ok((builder, None, identity))
        }
        None if std::io::stdin().is_terminal() => {
            // Seul `--config` arrive ici sans document : la page des réglages
            // n'a rien à lire derrière elle.
            Ok((
                DocumentBuilder::with_language(config.code_policy(), &config.default_language()),
                None,
                "configuration".to_string(),
            ))
        }
        None => {
            // Le tube est mis de côté et le descripteur 0 rendu au terminal,
            // faute de quoi le clavier resterait muet en mode pipe.
            let piped = input::detach_piped_stdin().context(crate::tr!("taking over the input stream", "récupération du flux d'entrée"))?;
            let stream = match piped {
                Some(file) => input::spawn_reader(file),
                None => input::spawn_reader(std::io::stdin()),
            };
            // Le flux est lu au fil de l'eau : sa langue est celle par défaut.
            Ok((
                DocumentBuilder::with_language(config.code_policy(), &config.default_language()),
                Some(stream),
                "stdin".to_string(),
            ))
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
        anyhow::bail!(crate::tr!(
            "--clip needs a binary built with the `clipboard` option",
            "--clip exige un binaire compilé avec l'option `clipboard`"
        ))
    }
}

/// Le passage à Kokoro au terminal : un Kokoro installé est adopté, une
/// seule fois ; absent, il s'installe seul (`App::install_default_voice`)
/// et rien n'est dit ici. Rend l'avis.
pub fn migrate_terminal(config: &mut Config) -> Option<String> {
    migrate_terminal_in(config, &crate::models::available(), crate::models::is_installed)
}

fn migrate_terminal_in(
    config: &mut Config,
    available: &[&'static crate::models::ModelPackage],
    installed: impl Fn(&crate::models::ModelPackage) -> bool,
) -> Option<String> {
    use crate::reader::migration::{former_voice, plan, Step, KOKORO_MIGRATION};
    let former = former_voice(config);
    match plan(config, available, installed) {
        Step::Switch(package) => {
            let path = crate::models::install_path(package)?;
            config.engine.model_path = path.to_string_lossy().into_owned();
            config.engine.family = package.family.to_string();
            // « system » ferait parler la voix du système : l'avis mentirait.
            if config.engine.engine == "system" {
                config.engine.engine = "auto".to_string();
            }
            config.align_voice_with_family();
            config.app.voice_migration = KOKORO_MIGRATION;
            Some(crate::tr!(
                en = "New voice: Kokoro replaces {former}. The former model stays installed.",
                fr = "Nouvelle voix : Kokoro remplace {former}. L'ancien modèle reste installé."
            ))
        }
        Step::Record => {
            config.app.voice_migration = KOKORO_MIGRATION;
            None
        }
        Step::Offer(_) => None,
        Step::Nothing => None,
    }
}

/// Le passage à Kokoro au lancement du terminal, sur `on_disk` (la
/// configuration du fichier). Rend ce qu'il faut écrire, s'il y a lieu. La
/// configuration de ce lancement suit le passage, sauf pour ce que la ligne
/// de commande a fixé (`--engine`, `--model`, `--family`).
fn migrate_for_launch(
    config: &mut Config,
    mut on_disk: Config,
    args: &Args,
    notices: &mut Vec<String>,
    available: &[&'static crate::models::ModelPackage],
    installed: impl Fn(&crate::models::ModelPackage) -> bool,
) -> Option<Config> {
    let before = on_disk.app.voice_migration;
    notices.extend(migrate_terminal_in(&mut on_disk, available, installed));
    if on_disk.app.voice_migration == before {
        return None;
    }
    config.app.voice_migration = on_disk.app.voice_migration;
    let switched = on_disk.engine.family != config.engine.family || on_disk.engine.model_path != config.engine.model_path;
    if switched {
        if args.engine.is_none() {
            config.engine.engine = on_disk.engine.engine.clone();
        }
        if args.model.is_none() {
            config.engine.model_path = on_disk.engine.model_path.clone();
        }
        if args.family.is_none() {
            config.engine.family = on_disk.engine.family.clone();
        }
        config.voice.per_language = on_disk.voice.per_language.clone();
    }
    Some(on_disk)
}

/// Adopte un modèle installé si la configuration n'en désigne aucun.
///
/// Sans modèle, l'installation automatique s'en charge (`App::install_default_voice`).
fn adopt_installed_model(config: &mut Config, notices: &mut Vec<String>) {
    let configured = config.engine.model_path.trim();
    if !configured.is_empty() {
        // Un chemin écrit par une version antérieure peut désigner un fichier
        // qui a déménagé — le répertoire de données a changé de nom. Le
        // garder ferait tomber sur la voix système alors qu'un modèle est là.
        if std::path::Path::new(&crate::config::expand_tilde(configured)).exists() {
            return;
        }
        notices.push(crate::tr!(en = "model not found: {configured}", fr = "modèle introuvable : {configured}"));
        config.engine.model_path.clear();
    }
    if let Some(package) = crate::models::first_installed() {
        if let Some(path) = crate::models::install_path(package) {
            config.engine.model_path = path.to_string_lossy().into_owned();
            config.engine.family = package.family.to_string();
        }
    }
}

fn apply_overrides(config: &mut Config, args: &Args) -> Result<()> {
    if let Some(v) = &args.voice {
        config.voice.voice = v.clone();
    }
    if let Some(v) = args.speed {
        config.voice.speed = v;
    }
    if let Some(v) = args.pitch {
        config.voice.pitch = v;
    }
    if let Some(v) = &args.language {
        config.voice.language = v.clone();
    }
    if let Some(v) = &args.engine {
        if tts::EnginePreference::parse(v).is_none() {
            anyhow::bail!(crate::tr!(en = "unknown engine: {v} (expected: auto or system)", fr = "moteur inconnu : {v} (attendu : auto ou system)"));
        }
        config.engine.engine = v.clone();
    }
    if let Some(v) = &args.model {
        config.engine.model_path = v.clone();
    }
    if let Some(v) = &args.family {
        config.engine.family = v.clone();
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
            anyhow::bail!(crate::tr!(
                en = "unknown value: {v} (expected: announce, skip or read)",
                fr = "valeur inconnue : {v} (attendu : announce, skip ou read)"
            ));
        }
        config.reading.code_blocks = v.clone();
    }
    if args.no_autoplay {
        config.reading.autoplay = false;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kokoro_int8() -> &'static crate::models::ModelPackage {
        crate::models::CATALOG.iter().find(|p| p.id == "kokoro_82m_int8").unwrap()
    }

    fn on_supertonic() -> Config {
        let mut config = Config::default();
        config.engine.family = "supertonic".into();
        config.engine.model_path = "/m/supertonic/supertonic-3-f16.gguf".into();
        config
    }

    #[test]
    fn an_offer_says_nothing_since_the_install_is_automatic() {
        let mut config = on_supertonic();
        assert_eq!(migrate_terminal_in(&mut config, &[kokoro_int8()], |_| false), None);
        assert_eq!(config.app.voice_migration, 0, "rien de retenu sans adoption");
        assert_eq!(config.engine.family, "supertonic");
    }

    #[test]
    fn the_terminal_adopts_an_installed_kokoro_once() {
        let mut config = on_supertonic();
        let notice = migrate_terminal_in(&mut config, &[kokoro_int8()], |_| true).expect("un avis");
        assert!(notice.starts_with("Nouvelle voix : Kokoro remplace supertonic."), "{notice}");
        assert!(!notice.contains("`s`"), "plus de rubrique Modèles où revenir : {notice}");
        assert_eq!(config.engine.family, "kokoro");
        assert!(config.engine.model_path.ends_with("kokoro/model_quantized.onnx"), "{}", config.engine.model_path);
        assert_eq!(config.voice.per_language["fr"], "p-fenrir60");
        assert_eq!(config.app.voice_migration, crate::reader::migration::KOKORO_MIGRATION);
        assert_eq!(migrate_terminal_in(&mut config, &[kokoro_int8()], |_| true), None, "une seule fois");
    }

    /// Une configuration du disque sur Supertonic, Kokoro int8 installé :
    /// ce que la ligne de commande garde pour ce lancement, ce qui s'écrit.
    fn migrated_with(args: &Args) -> (Config, Option<Config>, Vec<String>) {
        let mut config = on_supertonic();
        apply_overrides(&mut config, args).unwrap();
        let mut notices = Vec::new();
        let to_save = migrate_for_launch(&mut config, on_supertonic(), args, &mut notices, &[kokoro_int8()], |_| true);
        (config, to_save, notices)
    }

    #[test]
    fn the_command_line_keeps_its_engine_through_the_migration() {
        let args = Args { engine: Some("system".into()), ..Args::default() };
        let (config, to_save, notices) = migrated_with(&args);
        assert_eq!(config.engine.engine, "system", "--engine system tient pour ce lancement");
        let saved = to_save.expect("la migration s'écrit sur le disque");
        assert_eq!(saved.engine.family, "kokoro");
        assert_eq!(saved.app.voice_migration, crate::reader::migration::KOKORO_MIGRATION);
        assert_eq!(notices.len(), 1);
        // Sans option, le lancement prend Kokoro.
        let (config, _, _) = migrated_with(&Args::default());
        assert_eq!(config.engine.family, "kokoro");
    }

    #[test]
    fn the_command_line_keeps_its_model_and_family_through_the_migration() {
        let args = Args {
            model: Some("/ailleurs/magpie.gguf".into()),
            family: Some("magpie_tts".into()),
            ..Args::default()
        };
        let (config, to_save, _) = migrated_with(&args);
        assert_eq!(config.engine.model_path, "/ailleurs/magpie.gguf");
        assert_eq!(config.engine.family, "magpie_tts");
        assert_eq!(config.app.voice_migration, crate::reader::migration::KOKORO_MIGRATION);
        assert_eq!(to_save.expect("écrite").engine.family, "kokoro");
    }

    #[test]
    fn a_system_engine_on_disk_leaves_it_for_kokoro() {
        let mut config = on_supertonic();
        config.engine.engine = "system".into();
        migrate_terminal_in(&mut config, &[kokoro_int8()], |_| true).expect("un avis");
        assert_ne!(config.engine.engine, "system", "l'avis dit vrai : Kokoro parle");
    }

    #[test]
    fn the_terminal_without_kokoro_records_nothing() {
        let mut config = on_supertonic();
        assert_eq!(migrate_terminal_in(&mut config, &[], |_| true), None);
        assert_eq!(config.app.voice_migration, 0);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = crate::paths::test_scratch(&format!("launch-{name}"));
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
        assert_ne!(crate::position::document_identity(&a.join("spec.pdf")), crate::position::document_identity(&b.join("spec.pdf")));
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn the_same_file_has_one_identity_however_it_is_named() {
        let dir = scratch("c");
        std::fs::write(dir.join("notes.md"), "x").unwrap();
        let detour = dir.join("..").join(dir.file_name().unwrap()).join("notes.md");
        assert_eq!(crate::position::document_identity(&dir.join("notes.md")), crate::position::document_identity(&detour));
        std::fs::remove_dir_all(&dir).ok();
    }


    #[test]
    fn sessions_zero_on_the_command_line_means_auto() {
        let mut config = Config::default();
        config.engine.sessions = 3;
        let args = Args { sessions: Some(0), ..Args::default() };
        apply_overrides(&mut config, &args).unwrap();
        assert_eq!(config.engine.sessions, 0);
    }

    #[test]
    fn aligning_at_launch_fills_only_kokoros_voices() {
        // Kokoro remplit ses voix par langue en silence : rien d'inconnu.
        let mut config = Config::default();
        config.engine.family = "kokoro".into();
        config.voice.per_language.clear();
        assert!(config.align_voice_with_family());
        assert!(config.voice.per_language.contains_key("fr"), "rempli quand même");

        // Une famille hors catalogue ne nomme aucune voix : rien à réaligner.
        let mut config = Config::default();
        config.engine.family = "supertonic".into();
        config.voice.voice = "voix-qui-n-existe-pas".into();
        assert!(!config.align_voice_with_family());
        assert_eq!(config.voice.voice, "voix-qui-n-existe-pas");
    }

    #[test]
    fn nothing_to_read_only_from_a_bare_terminal() {
        assert!(nothing_to_read(&Args::default(), true), "sillage seul, dans un terminal");
        assert!(!nothing_to_read(&Args::default(), false), "cat notes.md | sillage lit le tube");
        let file = Args { file: Some("notes.md".into()), ..Args::default() };
        assert!(!nothing_to_read(&file, true));
        let clip = Args { clip: true, ..Args::default() };
        assert!(!nothing_to_read(&clip, true));
        let config = Args { config: true, ..Args::default() };
        assert!(!nothing_to_read(&config, true), "--config ouvre toujours les réglages");
    }

    #[test]
    fn the_short_help_names_the_four_ways_to_read() {
        for way in ["sillage document.md", "| sillage", "sillage --clip", "app", "--config", "--help"] {
            assert!(SHORT_HELP.contains(way), "{way} manque :\n{SHORT_HELP}");
        }
    }

    #[test]
    fn help_shows_only_the_everyday_options() {
        use clap::CommandFactory;
        let help = Args::command().render_help().to_string();
        for shown in ["[FILE]", "--clip", "--speed", "--voice", "--to-markdown", "--no-cache", "--config", "--update"] {
            assert!(help.contains(shown), "{shown} manque :\n{help}");
        }
        for hidden in [
            "--pitch", "--language", "--engine", "--model", "--family", "--sessions", "--prefetch",
            "--code-blocks", "--no-autoplay", "--write-config", "--show-config", "--check-kokoro",
        ] {
            assert!(!help.contains(hidden), "{hidden} se voit encore :\n{help}");
        }
    }

    #[test]
    fn hidden_options_are_still_accepted() {
        let args = Args::try_parse_from([
            "sillage", "--pitch", "0.5", "--language", "en", "--engine", "system", "--sessions", "2",
            "--prefetch", "4", "--code-blocks", "skip", "--no-autoplay", "--show-config",
        ])
        .expect("toujours acceptées");
        assert_eq!(args.pitch, Some(0.5));
        assert!(args.show_config);
    }
}

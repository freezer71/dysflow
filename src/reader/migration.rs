//! Le passage à Kokoro : une seule fois, au premier lancement après la mise à
//! jour, pour une configuration neuve comme pour une configuration qui nomme
//! déjà un modèle. Kokoro installé : il devient la voix active tout de suite.
//! Installable seulement : il est proposé, et rien ne se télécharge sans
//! accord. L'ancien modèle reste installé, et un avis le dit.

use std::time::Instant;

use anyhow::{anyhow, Result};

use crate::config::Config;
use crate::models::{self, ModelPackage, Progress};

use super::Reader;

/// La migration qui fait passer à Kokoro.
pub const KOKORO_MIGRATION: u32 = 1;

/// Ce que la migration fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Kokoro est installé : il devient le modèle actif.
    Switch(&'static ModelPackage),
    /// Kokoro est installable : il est proposé, sans rien télécharger ni
    /// retenir.
    Offer(&'static ModelPackage),
    /// Déjà sur Kokoro : seulement retenir que c'est fait.
    Record,
    /// Rien : déjà faite (ou refusée), ou pas de Kokoro dans ce binaire.
    Nothing,
}

/// La proposition de passer à Kokoro, en attente d'une réponse.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Offer {
    /// Le paquet proposé.
    pub id: String,
    /// Ce qu'il pèse à télécharger.
    pub bytes: u64,
    /// La voix qu'il remplacerait, pour l'avis de la fin.
    pub former: String,
    /// « Nouvelle voix disponible (326,4 Mo) — Passer à Kokoro ? » (taille
    /// écrite par `models::human_size`).
    pub message: String,
}

pub fn plan(config: &Config, available: &[&'static ModelPackage], installed: impl Fn(&ModelPackage) -> bool) -> Step {
    if config.app.voice_migration >= KOKORO_MIGRATION {
        return Step::Nothing;
    }
    let kokoro: Vec<&'static ModelPackage> = available.iter().copied().filter(|p| p.family == "kokoro").collect();
    let Some(first) = kokoro.first().copied() else {
        return Step::Nothing;
    };
    if config.engine.family == "kokoro" {
        return Step::Record;
    }
    match kokoro.iter().copied().find(|p| installed(p)) {
        Some(package) => Step::Switch(package),
        None => Step::Offer(first),
    }
}

/// Le nom de la voix en place, pour l'avis.
pub fn former_voice(config: &Config) -> String {
    let path = config.engine.model_path.trim();
    if path.is_empty() {
        return "la voix du système".to_string();
    }
    models::CATALOG
        .iter()
        .find(|p| models::is_active_package(path, p))
        .map(|p| p.display_name.to_string())
        .unwrap_or_else(|| config.engine.family.clone())
}

fn switched(former: &str) -> String {
    // Plus de « Réglages › Voix pour y revenir » : l'app ne propose plus
    // d'autre modèle que Kokoro (14 oct. 2026).
    format!("Nouvelle voix : Kokoro remplace {former}.")
}

fn offered(package: &ModelPackage) -> String {
    format!("Nouvelle voix disponible ({}) — Passer à Kokoro ?", models::human_size(package.total_bytes()))
}

impl Reader {
    /// Le passage à Kokoro, appelé une fois au lancement de l'app. Rend
    /// l'avis à montrer, s'il y en a un : le passage fait, ou la proposition
    /// (alors aussi dans `offer()`). Un lecteur hors disque ne bouge pas, ni
    /// un lecteur dont le `config.toml` n'a pu être lu : ses défauts
    /// écraseraient le fichier de l'utilisateur.
    pub fn migrate_voice(&mut self, now: Instant) -> Option<String> {
        if !self.persist || self.config_unreadable {
            return None;
        }
        let config = self.current_config().clone();
        let available = models::available_with(self.mlx);
        let former = former_voice(&config);
        match plan(&config, &available, models::is_installed) {
            Step::Nothing => None,
            Step::Record => {
                self.record_migration();
                None
            }
            Step::Switch(package) => {
                self.switch_to(package.id, now).ok()?;
                self.record_migration();
                let notice = switched(&former);
                self.notice = Some(notice.clone());
                Some(notice)
            }
            Step::Offer(package) => {
                let message = offered(package);
                self.offer = Some(Offer {
                    id: package.id.to_string(),
                    bytes: package.total_bytes(),
                    former,
                    message: message.clone(),
                });
                Some(message)
            }
        }
    }

    /// La proposition de passer à Kokoro, en attente d'une réponse.
    pub fn offer(&self) -> Option<&Offer> {
        self.offer.as_ref()
    }

    /// « Passer à Kokoro » : le téléchargement démarre, et Kokoro devient la
    /// voix active à sa fin — la migration n'est retenue qu'alors. Déjà
    /// installé entre-temps (onglet Modèles), il le devient tout de suite.
    pub fn accept_offer(&mut self, now: Instant) -> Result<()> {
        if self.config_unreadable {
            return Err(anyhow!("configuration illisible : le passage à Kokoro attend qu'elle soit corrigée"));
        }
        let offer = self.offer.clone().ok_or_else(|| anyhow!("aucune proposition en attente"))?;
        let package = self.package(&offer.id)?;
        if models::is_installed(package) {
            self.switch_to(package.id, now)?;
            self.offer = None;
            self.record_migration();
            self.notice = Some(switched(&offer.former));
            return Ok(());
        }
        // Déjà en téléchargement (onglet Modèles) : attendre sa fin.
        let downloading = self.download.as_ref().is_some_and(|d| {
            d.package.id == package.id && matches!(d.progress(), Progress::Running { .. } | Progress::Done)
        });
        if !downloading {
            self.install_voice(package.id)?;
        }
        self.offer = None;
        self.switch_when_downloaded = Some((offer.id, offer.former));
        Ok(())
    }

    /// « Plus tard » : le refus est retenu, la proposition ne revient pas.
    /// Kokoro reste installable depuis l'onglet Modèles.
    pub fn dismiss_offer(&mut self) {
        if self.offer.take().is_some() {
            self.record_migration();
        }
    }

    /// Jamais sur une configuration illisible : elle serait réécrite avec
    /// les défauts.
    fn record_migration(&mut self) {
        if self.config_unreadable {
            return;
        }
        self.sync_config();
        self.config.app.voice_migration = KOKORO_MIGRATION;
        if let Some(app) = self.session.as_mut() {
            app.config.app.voice_migration = KOKORO_MIGRATION;
        }
        let _ = self.save_config();
    }

    /// L'avis en attente (passage à Kokoro).
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }

    /// Le téléchargement accepté est fini : Kokoro devient la voix active,
    /// la migration est retenue, et l'avis le dit.
    pub(super) fn finish_migration(&mut self, downloaded: &str, now: Instant) {
        let Some((id, former)) = self.switch_when_downloaded.take() else { return };
        if id != downloaded {
            self.switch_when_downloaded = Some((id, former));
            return;
        }
        if self.switch_to(&id, now).is_ok() {
            self.record_migration();
            self.notice = Some(switched(&former));
        }
    }

    /// Le téléchargement accepté a échoué ou a été annulé : plus rien
    /// n'attend sa fin, rien n'est retenu, et la proposition revient.
    pub(super) fn abandon_migration(&mut self, ended: &str) {
        if !self.switch_when_downloaded.as_ref().is_some_and(|(id, _)| id == ended) {
            return;
        }
        let Some((id, former)) = self.switch_when_downloaded.take() else { return };
        if let Ok(package) = self.package(&id) {
            self.offer = Some(Offer { id, bytes: package.total_bytes(), former, message: offered(package) });
        }
    }

    /// Kokoro devient la voix active. « system » ferait parler la voix du
    /// système après le passage : l'avis mentirait. Le moteur repasse donc en
    /// « auto », qui suit la famille — et revient à « system » si la voix n'a
    /// pas pu changer, pour ne rien basculer en silence.
    fn switch_to(&mut self, id: &str, now: Instant) -> Result<()> {
        let left = self.set_engine_if("system", "auto");
        let before = self.config.engine.model_path.clone();
        let outcome = self.use_voice(id, now);
        if outcome.is_err() && left && self.config.engine.model_path == before {
            self.set_engine_if("auto", "system");
        }
        outcome
    }

    /// Remplace le moteur `from` par `to`, dans la configuration et le texte
    /// ouvert ; vrai si c'était `from`.
    fn set_engine_if(&mut self, from: &str, to: &str) -> bool {
        self.sync_config();
        if self.config.engine.engine != from {
            return false;
        }
        self.config.engine.engine = to.to_string();
        if let Some(app) = self.session.as_mut() {
            app.config.engine.engine = to.to_string();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::settings::SettingsPatch;
    use crate::audio::Player;
    use crate::models::{self, Download, Progress, CATALOG};
    use crate::tts::silent::SilentEngine;
    use crate::tts::TtsEngine;
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::Instant;

    /// Les tests qui posent ou retirent Kokoro dans le dossier isolé des
    /// modèles passent un par un : l'un le veut installé, l'autre absent.
    static MODELS: Mutex<()> = Mutex::new(());

    fn models_lock() -> MutexGuard<'static, ()> {
        MODELS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn by_id(id: &str) -> &'static ModelPackage {
        CATALOG.iter().find(|p| p.id == id).unwrap()
    }

    fn on_supertonic() -> Config {
        let mut c = Config::default();
        c.engine.family = "supertonic".into();
        c.engine.model_path = "/m/supertonic/supertonic-3-f16.gguf".into();
        c.voice.voice = "F2".into();
        c
    }

    /// Un paquet complet, creux, dans le dossier isolé des tests.
    fn install_hollow(package: &'static ModelPackage) {
        let root = models::models_dir().unwrap();
        for (_, path, bytes) in models::planned_files_in(&root, package) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(&path).unwrap().set_len(bytes).unwrap();
        }
    }

    #[test]
    fn an_installed_kokoro_is_taken_at_once() {
        let available = [by_id("kokoro_82m_mlx")];
        let step = plan(&on_supertonic(), &available, |p| p.family == "kokoro");
        assert_eq!(step, Step::Switch(by_id("kokoro_82m_mlx")));
    }

    #[test]
    fn an_installable_kokoro_is_offered() {
        let available = [by_id("kokoro_82m_mlx")];
        assert_eq!(plan(&on_supertonic(), &available, |_| false), Step::Offer(by_id("kokoro_82m_mlx")));
        // Une configuration neuve suit la même règle.
        assert_eq!(plan(&Config::default(), &available, |_| false), Step::Offer(by_id("kokoro_82m_mlx")));
    }

    #[test]
    fn nothing_is_recorded_without_kokoro_in_this_build() {
        assert_eq!(plan(&on_supertonic(), &[], |_| true), Step::Nothing);
    }

    #[test]
    fn a_config_already_on_kokoro_is_only_recorded() {
        let mut c = on_supertonic();
        c.engine.family = "kokoro".into();
        assert_eq!(plan(&c, &[by_id("kokoro_82m_mlx")], |_| true), Step::Record);
    }

    fn reader(config: Config) -> Reader {
        Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        )
    }

    /// Un lecteur dont les téléchargements ne touchent jamais le réseau.
    fn offline_reader(config: Config) -> Reader {
        let mut r = reader(config);
        r.set_mlx_compute(true);
        r.start_download = |package| Ok(Download::fake(package, false));
        r
    }

    /// Écrit la configuration dans un fichier temporaire et la relit, comme
    /// un nouveau lancement la trouverait.
    fn relaunched(config: &Config) -> Config {
        let dir = std::env::temp_dir().join(format!("sillon-migration-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("relance.toml");
        std::fs::write(&path, toml::to_string_pretty(config).unwrap()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).ok();
        toml::from_str::<Config>(&text).unwrap().normalised()
    }

    #[test]
    fn going_back_to_the_old_model_holds_after_a_relaunch() {
        let mut c = on_supertonic();
        c.app.voice_migration = KOKORO_MIGRATION;
        assert_eq!(plan(&c, &[by_id("kokoro_82m_mlx")], |_| true), Step::Nothing, "jamais deux fois");

        // De bout en bout : passage, retour à l'ancien modèle, relance.
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let mut r = offline_reader(on_supertonic());
        assert!(r.migrate_voice(Instant::now()).is_some());
        assert_eq!(r.config().engine.family, "kokoro");
        // Le retour tel qu'une ancienne version l'écrivait (Supertonic).
        let mut back = r.config().clone();
        back.engine.family = "supertonic".into();
        back.engine.model_path = on_supertonic().engine.model_path;
        back.align_voice_with_family();
        let mut again = offline_reader(relaunched(&back));
        assert_eq!(again.migrate_voice(Instant::now()), None, "la migration ne se refait pas");
        assert_eq!(again.config().engine.family, "supertonic", "le choix tient");
        assert_eq!(again.offer(), None);
        models::remove(mlx).unwrap();
    }

    #[test]
    fn the_former_voice_is_named_for_the_notice() {
        assert_eq!(former_voice(&Config::default()), "la voix du système");
        let mut c = Config::default();
        c.engine.family = "kokoro".into();
        c.engine.model_path = models::install_path(by_id("kokoro_82m_int8")).unwrap().to_string_lossy().into_owned();
        assert_eq!(former_voice(&c), "Kokoro 82M · int8");
        // Un modèle hors catalogue (Supertonic, retiré) : le nom de sa famille.
        c.engine.family = "supertonic".into();
        c.engine.model_path = "/m/supertonic/supertonic-3-f16.gguf".into();
        assert_eq!(former_voice(&c), "supertonic");
    }

    #[test]
    fn a_reader_off_disk_migrates_nothing() {
        let mut r = offline_reader(on_supertonic());
        r.set_persistent(false);
        assert_eq!(r.migrate_voice(Instant::now()), None);
        assert_eq!(r.config().app.voice_migration, 0);
        assert_eq!(r.offer(), None);
    }

    #[test]
    fn an_installed_kokoro_replaces_the_voice_and_says_so() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let mut r = offline_reader(on_supertonic());
        let notice = r.migrate_voice(Instant::now()).expect("un avis");
        assert!(notice.starts_with("Nouvelle voix : Kokoro remplace"), "{notice}");
        assert!(!notice.contains("pour y revenir"), "plus d'autre modèle à proposer : {notice}");
        assert_eq!(r.config().engine.family, "kokoro");
        assert_eq!(r.config().voice.voice, "F2", "l'ancienne voix reste pour y revenir");
        assert_eq!(r.config().voice.per_language["fr"], "p-fenrir60");
        assert_eq!(r.config().app.voice_migration, KOKORO_MIGRATION);
        assert_eq!(r.notice(), Some(notice.as_str()));
        assert_eq!(r.offer(), None);
        assert!(r.download().is_none(), "rien à télécharger");
        assert_eq!(r.migrate_voice(Instant::now()), None, "une seule fois");
        let snapshot = super::super::view::snapshot(&r);
        assert_eq!(snapshot.notice.as_deref(), Some(notice.as_str()));
        r.dismiss_notice();
        assert_eq!(r.notice(), None);
        models::remove(mlx).unwrap();
    }

    /// Sans Kokoro installé : la proposition, rien de téléchargé ni de retenu.
    fn offered() -> Reader {
        let mut r = offline_reader(on_supertonic());
        let notice = r.migrate_voice(Instant::now()).expect("une proposition");
        // La taille s'écrit comme dans l'onglet Modèles.
        let size = models::human_size(by_id("kokoro_82m_mlx").total_bytes());
        assert!(size.ends_with(" Mo"), "{size}");
        assert_eq!(notice, format!("Nouvelle voix disponible ({size}) — Passer à Kokoro ?"));
        r
    }

    #[test]
    fn an_installable_kokoro_is_only_offered() {
        let _guard = models_lock();
        let r = offered();
        let offer = r.offer().expect("proposition en attente");
        assert_eq!(offer.id, "kokoro_82m_mlx");
        assert!(offer.message.starts_with("Nouvelle voix disponible ("), "{}", offer.message);
        assert_eq!(offer.bytes, by_id("kokoro_82m_mlx").total_bytes());
        assert!(r.download().is_none(), "aucun téléchargement sans accord");
        assert_eq!(r.config().app.voice_migration, 0, "rien de retenu");
        assert_eq!(r.config().engine.family, "supertonic");
        assert_eq!(r.notice(), None);
        let snapshot = super::super::view::snapshot(&r);
        assert_eq!(snapshot.offer.as_ref().map(|o| o.id.as_str()), Some("kokoro_82m_mlx"));
    }

    #[test]
    fn an_accepted_offer_switches_when_the_download_ends() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let mut r = offered();
        r.accept_offer(Instant::now()).unwrap();
        assert_eq!(r.offer(), None);
        assert_eq!(r.download().map(|d| d.id), Some("kokoro_82m_mlx".to_string()));
        assert_eq!(r.config().app.voice_migration, 0, "retenue seulement à la fin");
        assert_eq!(r.config().engine.family, "supertonic");

        // Le téléchargement aboutit.
        install_hollow(mlx);
        r.download = Some(Download::fake(mlx, true));
        r.tick(Instant::now());
        assert_eq!(r.config().engine.family, "kokoro");
        assert_eq!(r.config().app.voice_migration, KOKORO_MIGRATION);
        let notice = r.notice().expect("un avis");
        assert!(notice.starts_with("Nouvelle voix : Kokoro remplace supertonic."), "{notice}");
        assert!(r.switch_when_downloaded.is_none());
        models::remove(mlx).unwrap();
    }

    #[test]
    fn a_failed_or_cancelled_download_switches_nothing_and_records_nothing() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        for ending in [Progress::Failed("réseau".into()), Progress::Cancelled] {
            let mut r = offered();
            r.accept_offer(Instant::now()).unwrap();
            r.download = Some(Download::fake_ended(mlx, ending.clone()));
            r.tick(Instant::now());
            assert!(r.switch_when_downloaded.is_none(), "{ending:?} : plus de passage en attente");
            assert_eq!(r.config().app.voice_migration, 0, "{ending:?} : rien de retenu");
            assert_eq!(r.config().engine.family, "supertonic");
            assert_eq!(r.notice(), None);
            assert_eq!(r.offer().map(|o| o.id.as_str()), Some("kokoro_82m_mlx"), "{ending:?} : proposée à nouveau");
        }
    }

    #[test]
    fn a_cancel_abandons_the_switch_at_once() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let mut r = offered();
        r.accept_offer(Instant::now()).unwrap();
        r.cancel_download().unwrap();
        assert!(r.switch_when_downloaded.is_none(), "plus rien n'attend la fin");
        assert!(r.offer().is_some(), "proposée à nouveau");
        // Les fichiers finissent quand même d'arriver : pas de passage à
        // Kokoro. (Le paquet peut encore devenir actif par la règle « aucune
        // voix active » : le faux chemin Supertonic n'en désigne aucune.)
        install_hollow(mlx);
        r.download = Some(Download::fake(mlx, true));
        r.tick(Instant::now());
        assert_eq!(r.config().app.voice_migration, 0, "rien de retenu");
        assert_eq!(r.notice(), None, "pas d'avis de passage");
        assert!(r.offer().is_some());
        models::remove(mlx).unwrap();
    }

    #[test]
    fn accepting_while_kokoro_already_downloads_waits_for_it() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let mut r = offered();
        // Lancé depuis l'onglet Modèles avant la réponse.
        r.install_voice(mlx.id).unwrap();
        r.accept_offer(Instant::now()).unwrap();
        assert_eq!(r.offer(), None);
        assert_eq!(r.switch_when_downloaded.as_ref().map(|(id, _)| id.as_str()), Some("kokoro_82m_mlx"));
        install_hollow(mlx);
        r.download = Some(Download::fake(mlx, true));
        r.tick(Instant::now());
        assert_eq!(r.config().engine.family, "kokoro");
        assert_eq!(r.config().app.voice_migration, KOKORO_MIGRATION);
        models::remove(mlx).unwrap();
    }

    #[test]
    fn a_system_engine_is_left_for_kokoro() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let mut c = on_supertonic();
        c.engine.engine = "system".into();
        let mut r = offline_reader(c);
        assert!(r.migrate_voice(Instant::now()).is_some());
        assert_ne!(r.config().engine.engine, "system", "l'avis dit vrai : Kokoro parle");
        models::remove(mlx).unwrap();
    }

    #[test]
    fn a_failed_switch_keeps_the_system_engine() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        let _ = models::remove(mlx);
        let mut c = on_supertonic();
        c.engine.engine = "system".into();
        let mut r = offline_reader(c);
        // Le téléchargement s'est dit fini, mais le paquet n'est pas là :
        // use_voice échoue, et « system » ne doit pas être devenu « auto ».
        r.switch_when_downloaded = Some((mlx.id.to_string(), "supertonic".into()));
        r.finish_migration(mlx.id, Instant::now());
        assert_eq!(r.config().engine.engine, "system", "rien n'a basculé : le moteur reste celui choisi");
        assert_eq!(r.config().engine.family, "supertonic");
        assert_eq!(r.notice(), None);
    }

    #[test]
    fn a_dismissed_offer_is_not_repeated() {
        let _guard = models_lock();
        let mut r = offered();
        r.dismiss_offer();
        assert_eq!(r.offer(), None);
        assert!(r.download().is_none());
        assert_eq!(r.config().app.voice_migration, KOKORO_MIGRATION, "le refus est retenu");
        assert_eq!(r.config().engine.family, "supertonic");
        assert_eq!(r.migrate_voice(Instant::now()), None);
        let mut again = offline_reader(relaunched(r.config()));
        assert_eq!(again.migrate_voice(Instant::now()), None, "ni au lancement suivant");
    }

    /// Un `config.toml` illisible : aucun chemin d'écriture du lecteur n'y
    /// touche — réglage, voix choisie, refus, fin d'un passage accepté,
    /// configuration du texte ouvert à la mise au repos ou à la fermeture.
    #[test]
    fn nothing_writes_over_an_unreadable_config() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let dir = std::env::temp_dir().join(format!("sillon-temoin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let witness = b"[voice\nvoice = \"F2\" # \xc3\xa9crit \xc3\xa0 la main".to_vec();
        std::fs::write(&path, &witness).unwrap();
        let mut r = offline_reader(on_supertonic());
        r.set_config_path(Some(path.clone()));
        r.set_config_unreadable(true);
        let t0 = Instant::now();
        r.update_settings(&SettingsPatch { keep_voice_minutes: Some(3), ..Default::default() }, t0).unwrap();
        r.use_voice(mlx.id, t0).unwrap();
        r.offer = Some(Offer { id: mlx.id.into(), bytes: 1, former: "F2".into(), message: String::new() });
        r.dismiss_offer();
        r.switch_when_downloaded = Some((mlx.id.into(), "F2".into()));
        r.finish_migration(mlx.id, t0);
        r.open_text("Un texte.", "a", t0);
        r.with_session(t0, |app| app.mark_config_dirty());
        r.close();
        r.rest();
        let after = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        models::remove(mlx).unwrap();
        assert_eq!(after, witness, "le fichier de l'utilisateur est intact");
    }

    /// Lisible, la configuration s'écrit bien au chemin donné (le témoin
    /// ci-dessus prouve donc quelque chose).
    #[test]
    fn a_readable_config_is_written_where_it_lives() {
        let dir = std::env::temp_dir().join(format!("sillon-lisible-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "ancien").unwrap();
        let mut r = offline_reader(on_supertonic());
        r.set_config_path(Some(path.clone()));
        r.update_settings(&SettingsPatch { keep_voice_minutes: Some(3), ..Default::default() }, Instant::now())
            .unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(after.contains("keep_voice_minutes = 3"), "{after}");
    }

    /// Un `config.toml` illisible (abîmé, écrit à la main) : le lecteur
    /// tourne sur les défauts, et le passage à Kokoro ne doit pas les écrire
    /// par-dessus le fichier de l'utilisateur.
    #[test]
    fn an_unreadable_config_is_never_migrated_over() {
        let _guard = models_lock();
        let mlx = by_id("kokoro_82m_mlx");
        install_hollow(mlx);
        let mut r = offline_reader(on_supertonic());
        r.set_config_unreadable(true);
        assert!(r.config_unreadable());
        assert_eq!(r.migrate_voice(Instant::now()), None, "rien ne bouge");
        assert_eq!(r.config().engine.family, "supertonic");
        assert_eq!(r.config().app.voice_migration, 0);
        assert_eq!(r.notice(), None);
        assert_eq!(r.offer(), None);
        models::remove(mlx).unwrap();

        // Une proposition déjà là (avant que la lecture n'échoue) : ni
        // l'accord ni le refus n'écrivent.
        let offer = Offer { id: mlx.id.into(), bytes: mlx.total_bytes(), former: "F2".into(), message: String::new() };
        r.offer = Some(offer.clone());
        assert!(r.accept_offer(Instant::now()).is_err());
        assert!(r.download().is_none(), "rien de téléchargé");
        assert_eq!(r.config().app.voice_migration, 0);
        r.offer = Some(offer);
        r.dismiss_offer();
        assert_eq!(r.offer(), None);
        assert_eq!(r.config().app.voice_migration, 0, "le refus n'est pas retenu");
    }

    #[test]
    fn accepting_without_an_offer_is_an_error() {
        let mut r = offline_reader(on_supertonic());
        assert!(r.accept_offer(Instant::now()).is_err());
        assert!(r.download().is_none());
    }
}

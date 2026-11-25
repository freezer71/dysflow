//! La page des réglages, le thème et les polices, en headless : un vrai
//! lecteur muet sur son fil (`silent_reader`, `Player::silent()`), la
//! plateforme de test de GPUI. Aucune fenêtre réelle, aucun son, aucun
//! téléchargement. Le seul `config.toml` écrit l'est dans un dossier
//! temporaire du test (feature `test-paths` du cœur, plus un chemin donné).
//!
//! La langue de l'interface est globale (`i18n::set_ui_lang`) : chaque essai
//! de ce fichier prend le même verrou, et celui qui la change la rétablit.

use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use dysflow::audio::Player;
use dysflow::config::{Config, Theme, READING_FONTS, READING_SIZE_MAX, READING_SIZE_MIN};
use dysflow::i18n::{set_ui_lang, ui_lang, UiLang};
use dysflow::reader::build::silent_reader;
use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::Reader;
use dysflow::tts::silent::SilentEngine;
use dysflow::tts::TtsEngine;
use dysflow_app::fonts;
use dysflow_app::session::Session;
use dysflow_app::settings::SettingsPage;
use dysflow_app::theme::{self, Appearance};
use dysflow_app::window::{MainWindow, Panel};
use gpui::{
    px, size, Entity, PlatformTextSystem, TestAppContext, TestDispatcher, VisualTestContext,
    WindowAppearance,
};

static SERIAL: Mutex<()> = Mutex::new(());

/// Un essai à la fois : la langue de l'interface est partagée.
fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Rétablit la langue de l'interface à la fin d'un essai, même s'il échoue.
struct LangGuard(UiLang);

impl LangGuard {
    fn keep() -> Self {
        LangGuard(ui_lang())
    }
}

impl Drop for LangGuard {
    fn drop(&mut self) {
        set_ui_lang(self.0);
    }
}

/// Un dossier temporaire propre au test, retiré à la fin.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dysflow-app-settings-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        Scratch(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session() -> Arc<Session> {
    Arc::new(Session::new(ReaderHandle::spawn(silent_reader)))
}

/// Un lecteur muet qui écrit son `config.toml` à `path` (dossier du test).
fn saving_session(path: PathBuf) -> Arc<Session> {
    Arc::new(Session::new(ReaderHandle::spawn(move || {
        let mut reader = silent_reader();
        reader.set_persistent(true);
        reader.set_config_path(Some(path));
        reader
    })))
}

/// Un lecteur muet dont le modèle est Kokoro : ses voix sont rangées par
/// langue. Rien ne se charge ni ne sonne (`SilentEngine`, `Player::silent`).
fn kokoro_session() -> Arc<Session> {
    Arc::new(Session::new(ReaderHandle::spawn(|| {
        let mut config = Config::default();
        config.reading.autoplay = false;
        config.app.keep_voice_minutes = -1;
        config.engine.family = "kokoro".into();
        let mut reader = Reader::new(
            config,
            Box::new(|_| Arc::new(SilentEngine) as Arc<dyn TtsEngine>),
            Box::new(|_: &str| Player::silent()),
            Instant::now(),
        );
        reader.set_persistent(false);
        reader
    })))
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    draw(cx);
}

/// La fenêtre principale dans une fenêtre de test, sous la racine de
/// gpui-component, comme dans `run`.
fn window<'a>(
    cx: &'a mut TestAppContext,
    session: &Arc<Session>,
) -> (Entity<MainWindow>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::default();
    let shared = Arc::clone(session);
    let out = Rc::clone(&slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let main = MainWindow::build(shared, None, window, cx);
        *out.borrow_mut() = Some(main.clone());
        gpui_component::Root::new(main, window, cx)
    });
    cx.simulate_resize(size(px(900.0), px(640.0)));
    cx.run_until_parked();
    draw(cx);
    let main = slot.borrow().clone().expect("la fenêtre principale");
    (main, cx)
}

fn command(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}

/// ⌘, (Ctrl+, hors du Mac) : la page s'ouvre et ses réglages arrivent.
fn open_settings(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Entity<SettingsPage> {
    cx.simulate_keystrokes(&command(","));
    settle(cx);
    assert_eq!(main.read_with(cx, |m, _| m.panel()), Some(Panel::Settings));
    let page = main.read_with(cx, |m, _| m.settings().clone());
    assert!(
        page.read_with(cx, |p, _| p.loaded()),
        "les réglages sont lus"
    );
    page
}

#[gpui::test]
fn every_change_is_saved_at_once(cx: &mut TestAppContext) {
    let _serial = serial();
    let dir = Scratch::new("saved");
    let path = dir.0.join("config.toml");
    let session = saving_session(path.clone());
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);

    page.update(cx, |p, cx| p.set_speed(1.5, cx));
    settle(cx);
    let saved = Config::read_from(&path).expect("config.toml est écrit tout de suite");
    assert_eq!(saved.voice.speed, 1.5);
    page.read_with(cx, |p, _| {
        assert_eq!(p.values().map(|v| v.speed), Some(1.5));
        assert_eq!(p.error(), None);
    });

    // Chaque réglage, sans bouton « Enregistrer ».
    page.update(cx, |p, cx| p.set_reading_font("openDyslexic", cx));
    settle(cx);
    let saved = Config::read_from(&path).unwrap();
    assert_eq!(saved.reading.font, "openDyslexic");
    page.update(cx, |p, cx| p.set_theme(Theme::Dark, cx));
    settle(cx);
    assert_eq!(Config::read_from(&path).unwrap().app.theme, Theme::Dark);
}

#[gpui::test]
fn a_saved_change_applies_to_the_reading_view(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    let reading = main.read_with(cx, |m, _| m.reading().clone());
    assert_eq!(reading.read_with(cx, |r, _| r.font_size()), 22.0);

    page.update(cx, |p, cx| p.set_reading_size(30.0, cx));
    settle(cx);
    assert_eq!(
        reading.read_with(cx, |r, _| r.font_size()),
        30.0,
        "la vue de lecture relit ses réglages"
    );
}

#[gpui::test]
fn voice_list_is_the_language_s(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = kokoro_session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    let all = session
        .ask(|r| r.capabilities().voices)
        .expect("le lecteur répond");
    assert!(all.iter().any(|v| v.language == "fr") && all.iter().any(|v| v.language == "en"));

    for language in ["en", "fr"] {
        page.update(cx, |p, cx| p.set_voice_language(language, cx));
        settle(cx);
        let listed = page.read_with(cx, |p, _| p.voice_options());
        let expected: Vec<_> = all
            .iter()
            .filter(|v| v.language == language)
            .cloned()
            .collect();
        assert!(!listed.is_empty(), "des voix en {language}");
        assert_eq!(listed, expected, "seules les voix en {language}");
    }

    // Choisir une voix l'écrit pour cette langue.
    page.update(cx, |p, cx| p.choose_voice("ff_siwis", cx));
    settle(cx);
    let chosen = session
        .ask(|r| r.settings().voices_by_language.get("fr").cloned())
        .flatten();
    assert_eq!(chosen.as_deref(), Some("ff_siwis"));
    assert_eq!(page.read_with(cx, |p, _| p.current_voice()), "ff_siwis");
}

#[gpui::test]
fn font_size_is_clamped(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    let slider = page.read_with(cx, |p, _| p.size_slider().clone());
    slider.read_with(cx, |s, _| {
        assert_eq!(s.min_value(), READING_SIZE_MIN);
        assert_eq!(s.max_value(), READING_SIZE_MAX);
    });

    page.update(cx, |p, cx| p.set_reading_size(8.0, cx));
    settle(cx);
    assert_eq!(
        page.read_with(cx, |p, _| p.values().unwrap().reading_size),
        14.0
    );
    assert_eq!(session.ask(|r| r.settings().reading_size), Some(14.0));

    page.update(cx, |p, cx| p.set_reading_size(99.0, cx));
    settle(cx);
    assert_eq!(
        page.read_with(cx, |p, _| p.values().unwrap().reading_size),
        40.0
    );
    assert_eq!(session.ask(|r| r.settings().reading_size), Some(40.0));
    assert_eq!(slider.read_with(cx, |s, _| s.value().end()), 40.0);
}

#[gpui::test]
fn ui_language_switches_live(cx: &mut TestAppContext) {
    let _serial = serial();
    let _lang = LangGuard::keep();
    set_ui_lang(UiLang::Fr);
    let session = session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    assert_eq!(page.read_with(cx, |p, _| p.title()), "Réglages");
    let before = main.read_with(cx, |m, _| m.renders());

    page.update(cx, |p, cx| p.set_ui_language("en", cx));
    settle(cx);
    assert_eq!(ui_lang(), UiLang::En, "par i18n::set_ui_lang");
    assert_eq!(page.read_with(cx, |p, _| p.title()), "Settings");
    assert!(
        main.read_with(cx, |m, _| m.renders()) > before,
        "toute la fenêtre se redessine dans la nouvelle langue"
    );
    assert_eq!(
        session.ask(|r| r.settings().ui_language).as_deref(),
        Some("en")
    );

    page.update(cx, |p, cx| p.set_ui_language("fr", cx));
    settle(cx);
    assert_eq!(ui_lang(), UiLang::Fr);
    assert_eq!(page.read_with(cx, |p, _| p.title()), "Réglages");
}

#[gpui::test]
fn theme_system_follows_appearance(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    let component_background = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| gpui_component::Theme::global(cx).background)
    };

    // « Système », le système clair : la palette claire, jusque dans les
    // composants de gpui-component.
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    assert_eq!(component_background(cx), theme::LIGHT.background());

    main.update_in(cx, |m, window, cx| {
        m.system_appearance_changed(WindowAppearance::Dark, window, cx)
    });
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Dark);
    assert_eq!(component_background(cx), theme::DARK.background());
    assert!(cx.update(|_, cx| gpui_component::Theme::global(cx).is_dark()));

    // Un thème choisi ne suit plus le système.
    let page = open_settings(&main, cx);
    page.update(cx, |p, cx| p.set_theme(Theme::Light, cx));
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    assert_eq!(component_background(cx), theme::LIGHT.background());

    // Sous un thème imposé, la fenêtre renvoie ce thème : ignoré.
    main.update_in(cx, |m, window, cx| {
        m.system_appearance_changed(WindowAppearance::Dark, window, cx)
    });
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);

    // Revenir à « Système » relit le système (clair, sur la plateforme de
    // test), puis le suit de nouveau.
    page.update(cx, |p, cx| p.set_theme(Theme::System, cx));
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    main.update_in(cx, |m, window, cx| {
        m.system_appearance_changed(WindowAppearance::Dark, window, cx)
    });
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Dark);
    assert_eq!(component_background(cx), theme::DARK.background());
}

#[gpui::test]
fn preview_never_plays_in_tests(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);

    page.update(cx, |p, cx| p.listen(cx));
    settle(cx);
    // L'aperçu est allé jusqu'au lecteur, qui n'a qu'une sortie muette
    // (`Player::silent()`) : il le refuse, rien ne sonne.
    let line = page.read_with(cx, |p, _| p.preview_line());
    assert_eq!(
        line.as_deref(),
        Some(dysflow::tts::preview::AUDIO_MISSING.get())
    );
    assert!(!session.snapshot().preview.playing);
}

#[gpui::test]
fn installing_a_voice_never_downloads_in_tests(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    // Une voix du catalogue que ce lecteur propose, sinon la première de
    // lecture (sans moteur compilé, la liste n'a que la voix du système).
    let id = page
        .read_with(cx, |p, _| p.models())
        .into_iter()
        .find(|m| !m.installed && !m.system)
        .map(|m| m.id)
        .unwrap_or_else(|| {
            dysflow::models::CATALOG
                .iter()
                .find(|m| m.kind == dysflow::models::Kind::Speech)
                .expect("une voix de lecture au catalogue")
                .id
                .to_string()
        });

    page.update(cx, |p, cx| p.install(&id, cx));
    settle(cx);
    // La requête est partie ; le lecteur hors disque la refuse sans rien
    // télécharger, et la page le dit.
    assert!(page.read_with(cx, |p, _| p.error().is_some()));
    assert!(session.snapshot().download.is_none());
}

#[gpui::test]
fn settings_open_and_close_from_the_keyboard(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    assert!(
        cx.debug_bounds("settings").is_some(),
        "le bouton ⚙ de la barre"
    );
    let page = open_settings(&main, cx);
    assert!(cx.debug_bounds("settings-page").is_some());
    assert!(
        page.update_in(cx, |p, window, cx| p
            .focus_handle()
            .contains_focused(window, cx)),
        "la page a le clavier"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(main.read_with(cx, |m, _| m.panel()), None);
    assert!(cx.debug_bounds("settings-page").is_none());
    // ⌘, rouvre, ⌘, referme.
    cx.simulate_keystrokes(&command(","));
    settle(cx);
    assert_eq!(main.read_with(cx, |m, _| m.panel()), Some(Panel::Settings));
    cx.simulate_keystrokes(&command(","));
    settle(cx);
    assert_eq!(main.read_with(cx, |m, _| m.panel()), None);
}

#[gpui::test]
fn opening_the_page_never_waits_for_the_reader(cx: &mut TestAppContext) {
    let _serial = serial();
    let session = session();
    let (main, cx) = window(cx, &session);
    // Le fil du lecteur est occupé (une ouverture longue, par exemple).
    session.send(Request::Ask(Box::new(|_| {
        std::thread::sleep(Duration::from_millis(600))
    })));
    let started = Instant::now();
    main.update_in(cx, |m, window, cx| m.open_settings(window, cx));
    assert!(started.elapsed() < Duration::from_millis(200));
    let page = main.read_with(cx, |m, _| m.settings().clone());
    assert!(
        !page.read_with(cx, |p, _| p.loaded()),
        "en attendant, rien n'est lu"
    );
    settle(cx);
    assert!(page.read_with(cx, |p, _| p.loaded()));
}

/// Un système de texte sans aucune police : seules celles que l'app
/// enregistre y sont. Les familles se lisent dans la table `name` des
/// fichiers reçus ; le reste est celui de la plateforme de test.
#[derive(Default)]
struct BareTextSystem {
    families: Mutex<Vec<String>>,
}

impl PlatformTextSystem for BareTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> gpui::Result<()> {
        let mut families = self.families.lock().unwrap();
        for font in fonts {
            families.push(family_name(&font).expect("une table name lisible"));
        }
        Ok(())
    }

    fn all_font_names(&self) -> Vec<String> {
        self.families.lock().unwrap().clone()
    }

    fn font_id(&self, descriptor: &gpui::Font) -> gpui::Result<gpui::FontId> {
        gpui::NoopTextSystem.font_id(descriptor)
    }

    fn font_metrics(&self, font_id: gpui::FontId) -> gpui::FontMetrics {
        gpui::NoopTextSystem.font_metrics(font_id)
    }

    fn typographic_bounds(
        &self,
        font_id: gpui::FontId,
        glyph_id: gpui::GlyphId,
    ) -> gpui::Result<gpui::Bounds<f32>> {
        gpui::NoopTextSystem.typographic_bounds(font_id, glyph_id)
    }

    fn advance(
        &self,
        font_id: gpui::FontId,
        glyph_id: gpui::GlyphId,
    ) -> gpui::Result<gpui::Size<f32>> {
        gpui::NoopTextSystem.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: gpui::FontId, ch: char) -> Option<gpui::GlyphId> {
        gpui::NoopTextSystem.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(
        &self,
        params: &gpui::RenderGlyphParams,
    ) -> gpui::Result<gpui::Bounds<gpui::DevicePixels>> {
        gpui::NoopTextSystem.glyph_raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        params: &gpui::RenderGlyphParams,
        raster_bounds: gpui::Bounds<gpui::DevicePixels>,
    ) -> gpui::Result<(gpui::Size<gpui::DevicePixels>, Vec<u8>)> {
        gpui::NoopTextSystem.rasterize_glyph(params, raster_bounds)
    }

    fn layout_line(
        &self,
        text: &str,
        font_size: gpui::Pixels,
        runs: &[gpui::FontRun],
    ) -> gpui::LineLayout {
        gpui::NoopTextSystem.layout_line(text, font_size, runs)
    }

    fn recommended_rendering_mode(
        &self,
        font_id: gpui::FontId,
        font_size: gpui::Pixels,
    ) -> gpui::TextRenderingMode {
        gpui::NoopTextSystem.recommended_rendering_mode(font_id, font_size)
    }
}

/// La famille d'une police TrueType ou OpenType : l'identifiant 16
/// (famille typographique) s'il y est, sinon le 1.
fn family_name(font: &[u8]) -> Option<String> {
    let u16_at = |at: usize| Some(u16::from_be_bytes(font.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_be_bytes(font.get(at..at + 4)?.try_into().ok()?));
    let tables = u16_at(4)? as usize;
    let name = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&at| font.get(at..at + 4) == Some(b"name"))
        .and_then(|at| u32_at(at + 8))? as usize;
    let count = u16_at(name + 2)? as usize;
    let strings = name + u16_at(name + 4)? as usize;
    let mut found: Vec<(u16, String)> = Vec::new();
    for i in 0..count {
        let record = name + 6 + 12 * i;
        let (platform, id) = (u16_at(record)?, u16_at(record + 6)?);
        let (length, offset) = (u16_at(record + 8)? as usize, u16_at(record + 10)? as usize);
        let bytes = font.get(strings + offset..strings + offset + length)?;
        let text = match platform {
            // Windows : UTF-16 gros-boutiste.
            3 | 0 => String::from_utf16(
                &bytes
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                    .collect::<Vec<_>>(),
            )
            .ok()?,
            1 => bytes.iter().map(|&b| b as char).collect(),
            _ => continue,
        };
        if id == 1 || id == 16 {
            found.push((id, text));
        }
    }
    found.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
    found.into_iter().next().map(|(_, text)| text)
}

#[test]
fn fonts_are_embedded() {
    let _serial = serial();
    let text_system = Arc::new(BareTextSystem::default());
    let cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::clone(&text_system) as Arc<dyn PlatformTextSystem>,
    );
    assert!(
        cx.update(|cx| cx.text_system().all_font_names()).is_empty(),
        "aucune police du système"
    );
    cx.update(|cx| dysflow_app::init(cx, Theme::System));
    let names = cx.update(|cx| cx.text_system().all_font_names());
    for id in READING_FONTS {
        let family = fonts::family(id);
        assert!(
            names.iter().any(|n| n == family),
            "{family} est embarquée : {names:?}"
        );
    }
    for family in ["Literata", "Atkinson Hyperlegible", "OpenDyslexic"] {
        assert!(names.iter().any(|n| n == family), "{family} : {names:?}");
    }
    // Les quatre styles d'Atkinson et d'OpenDyslexic, les deux de Literata
    // (variable) : rien ne manque pour le gras ni l'italique.
    assert_eq!(fonts::EMBEDDED.len(), 10);
    cx.quit();
}

/// Deux changements de suite, Sombre puis Clair : leurs réponses reviennent
/// par deux tâches d'arrière-plan, sans ordre garanti. Si celle de Clair
/// arrive d'abord, celle de Sombre ne doit rien réappliquer à la fenêtre.
#[gpui::test]
fn an_older_answer_never_overrides_a_newer_setting(cx: &mut TestAppContext) {
    let _serial = serial();
    let scratch = Scratch::new("out-of-order");
    let path = scratch.0.join("config.toml");
    let session = saving_session(path.clone());
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    // Le lecteur est occupé : les vraies réponses attendent.
    session.send(Request::Ask(Box::new(|_| {
        std::thread::sleep(Duration::from_millis(300))
    })));
    page.update(cx, |p, cx| p.set_theme(Theme::Dark, cx));
    let dark = page.read_with(cx, |p, _| p.last_sent());
    page.update(cx, |p, cx| p.set_theme(Theme::Light, cx));
    let light = page.read_with(cx, |p, _| p.last_sent());
    let base = page.read_with(cx, |p, _| p.values().cloned().unwrap());
    let mut light_view = base.clone();
    light_view.theme = Theme::Light;
    let mut dark_view = base;
    dark_view.theme = Theme::Dark;

    // Dans le désordre : la plus récente d'abord.
    page.update(cx, |p, cx| p.deliver_saved(light, light_view, cx));
    page.update(cx, |p, cx| p.deliver_saved(dark, dark_view, cx));
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    assert_eq!(
        page.read_with(cx, |p, _| p.values().unwrap().theme),
        Theme::Light
    );

    // Puis les vraies réponses, dans l'ordre du lecteur : rien ne bouge.
    settle(cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    assert_eq!(Config::read_from(&path).unwrap().app.theme, Theme::Light);
}

/// Le fil du lecteur occupé plus de 5 s (une ouverture longue, l'OCR, le
/// chargement d'une voix) : le réglage attend, « Enregistrement… », puis
/// s'applique à la fenêtre quand la réponse arrive.
#[gpui::test]
fn a_slow_reader_still_applies_the_setting_to_the_window(cx: &mut TestAppContext) {
    let _serial = serial();
    let scratch = Scratch::new("slow-reader");
    let path = scratch.0.join("config.toml");
    let session = saving_session(path.clone());
    let (main, cx) = window(cx, &session);
    let page = open_settings(&main, cx);
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Light);
    session.send(Request::Ask(Box::new(|_| {
        std::thread::sleep(Duration::from_millis(5_500))
    })));
    page.update(cx, |p, cx| p.set_theme(Theme::Dark, cx));
    assert!(page.read_with(cx, |p, _| p.saving()), "« Enregistrement… »");
    draw(cx);
    assert!(
        cx.debug_bounds("settings-saving").is_some(),
        "l'attente se voit"
    );

    settle(cx);
    page.read_with(cx, |p, _| {
        assert!(!p.saving());
        assert_eq!(p.error(), None);
    });
    assert_eq!(cx.update(|_, cx| theme::appearance(cx)), Appearance::Dark);
    assert_eq!(Config::read_from(&path).unwrap().app.theme, Theme::Dark);
}

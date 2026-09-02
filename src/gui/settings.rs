//! La configuration en widgets : rubriques à gauche, réglages à droite.
//!
//! Le terminal montrait une page à la fois faute de place ; la fenêtre a la
//! place, et les deux pages redeviennent une liste et un contenu. Les valeurs
//! prennent effet immédiatement, par les mêmes `Effect` qu'au terminal.

use std::time::{Duration, Instant};

use eframe::egui::{self, ComboBox, ProgressBar, RichText, ScrollArea, Slider, TextEdit, Ui};

use super::fonts;
use super::icons::Icon;
use super::widgets::{self, Button, NavRow};
use super::theme::Palette;
use super::window::Gui;
use crate::app::config::{Control, Field, Page, Section};
use crate::config::GuiConfig;
use crate::models;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rubric {
    /// Une rubrique du contrôleur, par son rang dans `Section::ALL`.
    Section(usize),
    /// Police, taille, interlignage, thème : la fenêtre seule.
    Display,
}

/// Largeur de la colonne des libellés.
const LABEL_W: f32 = 200.0;

/// Les rubriques, dans la barre latérale : elles remplacent le sommaire tant
/// que l'écran de réglages est ouvert.
pub fn rubrics(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    widgets::group_label(ui, palette, "Réglages");
    for (i, section) in Section::ALL.iter().enumerate() {
        let selected = gui.rubric == Rubric::Section(i);
        let summary = section.summary(&gui.app.config);
        if NavRow::new(section.label())
            .selected(selected)
            .show(ui, palette)
            .on_hover_text(summary)
            .clicked()
        {
            gui.rubric = Rubric::Section(i);
            gui.app.screen.section = i;
            gui.app.screen.page = Page::Detail;
            gui.app.screen.row = 0;
        }
    }
    let display = format!("{} · {:.0} px", gui.app.config.gui.font, gui.app.config.gui.font_size());
    if NavRow::new("Affichage")
        .selected(gui.rubric == Rubric::Display)
        .show(ui, palette)
        .on_hover_text(display)
        .clicked()
    {
        gui.rubric = Rubric::Display;
    }
    ui.add_space(8.0);
    if NavRow::new("Retour à la lecture")
        .icon(Icon::Prev)
        .show(ui, palette)
        .on_hover_text("Échap · s")
        .clicked()
    {
        gui.app.toggle_config();
    }
}

/// Le contenu de la rubrique ouverte, avec son pied.
pub fn show(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    footer(ui, gui, palette);
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match gui.rubric {
            Rubric::Section(i) if Section::ALL[i] == Section::Models => models_list(ui, gui, palette),
            Rubric::Section(i) => fields(ui, gui, Section::ALL[i], palette),
            Rubric::Display => display(ui, gui, palette),
        });
}

fn labelled(ui: &mut Ui, palette: &Palette, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_W, 22.0],
            egui::Label::new(RichText::new(label).color(palette.normal)),
        );
        add(ui);
    });
}

fn fields(ui: &mut Ui, gui: &mut Gui, section: Section, palette: &Palette) {
    ui.heading(RichText::new(section.label()).color(palette.bright));
    ui.add_space(8.0);
    let show_all = gui.app.screen.show_all;
    let mut hidden = 0;
    for field in section.fields() {
        // La fenêtre a son propre interlignage, en fraction de ligne.
        if *field == Field::LineSpacing {
            continue;
        }
        let ignored = field.is_ignored(&gui.app.config);
        if ignored && !show_all {
            hidden += 1;
            continue;
        }
        ui.add_enabled_ui(!ignored, |ui| {
            labelled(ui, palette, field.label(), |ui| {
                widget(ui, gui, *field);
                if ignored {
                    ui.label(RichText::new("(ignoré par ce modèle)").color(palette.faint));
                }
            });
        });
        ui.label(RichText::new(field.hint_for(&gui.app.config)).small().color(palette.dim));
        ui.add_space(10.0);
    }
    ui.separator();
    let mut show_all = gui.app.screen.show_all;
    let label = if hidden > 0 {
        format!("Afficher les {hidden} réglage(s) sans effet sur ce modèle")
    } else {
        "Afficher les réglages sans effet".to_string()
    };
    if ui.checkbox(&mut show_all, label).changed() {
        gui.app.screen.show_all = show_all;
    }
}

/// Le widget d'un réglage, choisi par ce qu'il est.
fn widget(ui: &mut Ui, gui: &mut Gui, field: Field) {
    let volume = gui.app.player.volume();
    match field.control(&gui.app.config) {
        Control::Slider { min, max, step, decimals, unset } => {
            let current = field.number(&gui.app.config, volume);
            if let Some((label, start)) = unset {
                let mut left = current.is_none();
                if ui.checkbox(&mut left, label).changed() {
                    gui.app.config_set_number(field, if left { None } else { Some(start) });
                    return;
                }
            }
            let Some(mut value) = current else { return };
            // Le volume se lit en pour cent.
            let percent = field == Field::Volume;
            let (lo, hi, st) = if percent {
                value *= 100.0;
                (min * 100.0, max * 100.0, step * 100.0)
            } else {
                (min, max, step)
            };
            // Le curseur arrondit au pas dès le premier dessin : seul un vrai
            // déplacement compte.
            let before = value;
            let mut slider = Slider::new(&mut value, lo..=hi)
                .step_by(st)
                .fixed_decimals(if percent { 0 } else { decimals });
            slider = match field {
                Field::Volume => slider.suffix(" %"),
                Field::Speed => slider.suffix("×"),
                _ => slider,
            };
            if ui.add(slider).changed() && (value - before).abs() > 1e-9 {
                gui.app
                    .config_set_number(field, Some(if percent { value / 100.0 } else { value }));
            }
        }
        Control::Integer { min, max, unset, zero } => {
            let current = field.number(&gui.app.config, volume);
            if let Some((label, start)) = unset {
                let mut left = current.is_none();
                if ui.checkbox(&mut left, label).changed() {
                    gui.app
                        .config_set_number(field, if left { None } else { Some(start as f64) });
                    return;
                }
            }
            let Some(mut value) = current else { return };
            if let Some(label) = zero {
                let mut at_zero = value == 0.0;
                if ui.checkbox(&mut at_zero, label).changed() {
                    gui.app
                        .config_set_number(field, Some(if at_zero { 0.0 } else { min as f64 }));
                    return;
                }
                if at_zero {
                    return;
                }
            }
            let before = value;
            if ui
                .add(Slider::new(&mut value, min as f64..=max as f64).integer())
                .changed()
                && (value - before).abs() > 1e-9
            {
                gui.app.config_set_number(field, Some(value));
            }
        }
        Control::Choice(choices) => {
            let current = choice_of(field, &gui.app.config);
            let shown = if current.is_empty() { "(défaut)" } else { current.as_str() };
            let mut picked: Option<&str> = None;
            ComboBox::from_id_salt(field.label())
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for choice in choices {
                        let label = if choice.is_empty() { "(défaut)" } else { choice };
                        if ui.selectable_label(*choice == current, label).clicked() {
                            picked = Some(choice);
                        }
                    }
                });
            if let Some(choice) = picked {
                gui.app.config_set_choice(field, choice);
            }
        }
        Control::Voices(voices) => {
            let current = gui.app.config.voice.voice.clone();
            let shown = voices
                .iter()
                .find(|v| v.id == current)
                .map_or(voices[0].label, |v| v.label);
            let mut picked: Option<&str> = None;
            ComboBox::from_id_salt("voix")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for voice in voices {
                        if ui.selectable_label(voice.id == current, voice.label).clicked() {
                            picked = Some(voice.id);
                        }
                    }
                });
            if let Some(id) = picked {
                gui.app.config_set_choice(field, id);
            }
        }
        Control::Toggle => {
            let mut on = field.value(&gui.app.config, volume) == "oui";
            if ui.checkbox(&mut on, "").changed() {
                let effect = field.adjust(&mut gui.app.config, 1);
                gui.app.apply_effect(effect);
            }
        }
        Control::Text => {
            let key = field.label();
            let config_text = field.raw_text(&gui.app.config);
            let buffer = gui.edits.entry(key).or_insert(config_text);
            let response = ui.add(TextEdit::singleline(buffer).desired_width(360.0));
            // La valeur part quand le champ rend le clavier — Entrée ou un
            // clic ailleurs —, pas à chaque lettre : un chemin de modèle
            // recharge le moteur.
            if response.lost_focus() {
                let text = buffer.clone();
                gui.edits.remove(key);
                if text.trim() != field.raw_text(&gui.app.config).trim() {
                    gui.app.config_set_text(field, text);
                }
            }
            if field == Field::ModelPath && ui.button("Parcourir…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    gui.edits.remove(key);
                    gui.app
                        .config_set_text(field, path.to_string_lossy().into_owned());
                }
            }
        }
    }
}

fn choice_of(field: Field, config: &crate::config::Config) -> String {
    match field {
        Field::Language => config.voice.language.clone(),
        Field::Emotion => config.voice.emotion.clone(),
        Field::CodeBlocks => config.reading.code_blocks.clone(),
        Field::EnginePref => config.engine.engine.clone(),
        Field::Compute => config.engine.compute.clone(),
        _ => String::new(),
    }
}

/// Police, taille, interlignage, thème : ce que la fenêtre seule règle.
fn display(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.heading(RichText::new("Affichage").color(palette.bright));
    ui.add_space(8.0);
    let gui_config = &mut gui.app.config.gui;

    labelled(ui, palette, "Police de lecture", |ui| {
        let names = [
            (fonts::ATKINSON, "Atkinson Hyperlegible"),
            (fonts::OPENDYSLEXIC, "OpenDyslexic"),
            (fonts::DEFAULT, "Standard"),
        ];
        let shown = names
            .iter()
            .find(|(id, _)| *id == gui_config.font)
            .map_or("Standard", |(_, n)| *n);
        ComboBox::from_id_salt("police")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (id, name) in names {
                    if ui.selectable_label(gui_config.font == id, name).clicked() {
                        gui_config.font = id.to_string();
                    }
                }
            });
    });
    ui.label(
        RichText::new(
            "Atkinson Hyperlegible distingue les glyphes qui se confondent — I l 1, O 0, b d. \
             OpenDyslexic est celle qu'on demande par son nom : à chacun de juger.",
        )
        .small()
        .color(palette.dim),
    );
    ui.add_space(10.0);

    labelled(ui, palette, "Taille du texte", |ui| {
        let mut size = gui_config.font_size();
        if ui
            .add(Slider::new(&mut size, 14.0..=40.0).step_by(1.0).fixed_decimals(0).suffix(" px"))
            .changed()
        {
            gui_config.font_size = size;
        }
    });
    ui.add_space(10.0);

    labelled(ui, palette, "Interlignage", |ui| {
        let mut line_height = gui_config.line_height();
        if ui
            .add(Slider::new(&mut line_height, 1.0..=2.5).step_by(0.05).fixed_decimals(2))
            .changed()
        {
            gui_config.line_height = line_height;
        }
    });
    ui.label(
        RichText::new(
            "En fraction de la taille du texte. 1,6 est la recommandation courante pour la \
             lecture accompagnée.",
        )
        .small()
        .color(palette.dim),
    );
    ui.add_space(10.0);

    labelled(ui, palette, "Thème", |ui| {
        for (id, name) in GuiConfig::THEMES.iter().zip(["Sombre", "Clair"]) {
            if ui.selectable_label(gui_config.theme == *id, name).clicked() {
                gui_config.theme = id.to_string();
            }
        }
    });
    ui.add_space(10.0);
    ui.label(
        RichText::new(
            "La largeur de colonne et la règle de lecture se règlent dans « Lecture » : elles \
             valent pour le terminal aussi.",
        )
        .small()
        .color(palette.dim),
    );
}

fn models_list(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.heading(RichText::new("Modèles").color(palette.bright));
    ui.add_space(8.0);
    let packages = models::available();
    if packages.is_empty() {
        ui.label(RichText::new("Aucun modèle disponible.").strong());
        ui.label(
            RichText::new(
                "Ce binaire a été compilé sans audio.cpp ; la lecture utilise la voix du \
                 système. Pour un vrai modèle :",
            )
            .color(palette.dim),
        );
        ui.code("cargo build --release --features gui,audio-cpp");
        return;
    }
    // Les actions du contrôleur portent sur la rubrique courante et sa ligne.
    gui.app.screen.section = 0;
    gui.app.screen.page = Page::Detail;
    let active = gui.app.config.engine.model_path.clone();
    let downloading = gui
        .app
        .screen
        .download
        .as_ref()
        .map(|d| (d.package.id, d.progress()));

    for (i, package) in packages.iter().enumerate() {
        let installed = models::is_installed(package);
        let truncated = models::is_truncated(package);
        let is_active =
            models::install_path(package).is_some_and(|p| p.to_string_lossy() == active);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let name = if is_active {
                    RichText::new(package.display_name).color(palette.accent).strong()
                } else {
                    RichText::new(package.display_name).color(palette.bright).strong()
                };
                ui.label(name);
                let (badge, color) = if truncated {
                    ("⚠ tronqué", palette.accent)
                } else if is_active {
                    ("● utilisé", palette.accent)
                } else if installed {
                    ("○ installé", palette.normal)
                } else {
                    ("à venir", palette.faint)
                };
                ui.label(RichText::new(badge).color(color));
                ui.label(RichText::new(models::human_size(package.bytes)).color(palette.dim));
                if package.recommended && !installed {
                    ui.label(RichText::new("Recommandé pour commencer").color(palette.accent));
                }
            });
            ui.label(RichText::new(package.description).small().color(palette.dim));
            ui.label(RichText::new(package.languages_label).small().color(palette.faint));
            // Ce que ça donnera à l'oreille : un modèle lent n'est pas en panne.
            let slow = package.pace.starts_with("Plus lent");
            ui.label(
                RichText::new(package.pace)
                    .small()
                    .color(if slow { palette.accent } else { palette.dim }),
            );
            if truncated {
                ui.label(
                    RichText::new(format!(
                        "Fichier incomplet : {} au lieu de {}.",
                        models::human_size(models::installed_size(package).unwrap_or(0)),
                        models::human_size(package.bytes)
                    ))
                    .color(palette.accent),
                );
            }
            ui.horizontal(|ui| match &downloading {
                Some((id, models::Progress::Running { received, total })) if *id == package.id => {
                    let ratio = if *total > 0 { *received as f32 / *total as f32 } else { 0.0 };
                    ui.add(ProgressBar::new(ratio).desired_width(320.0).text(format!(
                        "{} / {}",
                        models::human_size(*received),
                        models::human_size(*total)
                    )));
                    if Button::icon(Icon::Stop).label("Interrompre").show(ui, palette).clicked() {
                        gui.app.screen.row = i;
                        gui.app.config_delete();
                    }
                }
                _ => {
                    if installed && !truncated {
                        if !is_active
                            && Button::icon(Icon::Check).label("Utiliser").show(ui, palette).clicked()
                        {
                            gui.app.screen.row = i;
                            gui.app.config_activate();
                        }
                        delete_button(ui, gui, i, palette);
                    } else {
                        let busy = downloading.is_some();
                        let label = if truncated { "Réinstaller" } else { "Installer" };
                        let clicked = ui
                            .add_enabled_ui(!busy, |ui| {
                                Button::icon(Icon::Download).label(label).show(ui, palette).clicked()
                            })
                            .inner;
                        if clicked {
                            gui.app.screen.row = i;
                            gui.app.config_activate();
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
    }
    if let Some(free) = models::free_space() {
        ui.label(
            RichText::new(format!("Espace disque libre : {}", models::human_size(free)))
                .color(palette.faint),
        );
    }
}

/// Supprimer demande un second clic dans les trois secondes : à la souris,
/// un clic de travers effacerait 1,6 Go.
fn delete_button(ui: &mut Ui, gui: &mut Gui, row: usize, palette: &Palette) {
    let armed = gui
        .confirm_delete
        .is_some_and(|(r, at)| r == row && at.elapsed() < Duration::from_secs(3));
    if armed {
        if Button::icon(Icon::Trash)
            .label("Confirmer la suppression")
            .primary()
            .show(ui, palette)
            .clicked()
        {
            gui.confirm_delete = None;
            gui.app.screen.row = row;
            gui.app.config_delete();
        }
    } else if Button::icon(Icon::Trash).label("Supprimer").show(ui, palette).clicked() {
        gui.confirm_delete = Some((row, Instant::now()));
    }
}

/// Écouter, enregistrer, et le message de la rubrique.
fn footer(ui: &mut Ui, gui: &mut Gui, palette: &Palette) {
    ui.horizontal(|ui| {
        if Button::icon(Icon::Speaker)
            .label("Écouter la voix")
            .show(ui, palette)
            .on_hover_text("t")
            .clicked()
        {
            // Dans la liste des modèles, l'aperçu porte sur celui qui est chargé.
            if gui.rubric == Rubric::Section(0) {
                let active = gui.app.config.engine.model_path.clone();
                if let Some(i) = models::available().iter().position(|p| {
                    models::install_path(p).is_some_and(|q| q.to_string_lossy() == active)
                }) {
                    gui.app.screen.row = i;
                }
            }
            gui.app.preview_voice();
        }
        if Button::icon(Icon::Check)
            .label("Enregistrer")
            .show(ui, palette)
            .on_hover_text("w")
            .clicked()
        {
            gui.app.save_config();
        }
        // L'aperçu passe devant : c'est le seul message qui décrit une action
        // en train de se produire.
        let message = if let Some(preview) = gui.app.preview_status() {
            preview
        } else if gui.app.reference_missing() {
            "⚠ ce modèle ne parle qu'en clonant : renseignez un WAV dans « Voix »".to_string()
        } else if !gui.app.screen.notice.is_empty() {
            gui.app.screen.notice.clone()
        } else {
            gui.app.status().to_string()
        };
        if !message.is_empty() {
            ui.label(RichText::new(message).color(palette.accent));
        }
    });
    ui.separator();
}

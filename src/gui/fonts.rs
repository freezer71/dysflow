//! Polices embarquées : la fenêtre ne dépend pas de celles de la machine.
//!
//! egui ne connaît pas la graisse : chaque face est une famille à part, et
//! `family` choisit la bonne pour un mot en gras, en italique, ou les deux.
//! Les polices d'origine restent en repli, pour les symboles et les emoji.
//! Une police pixel s'ajoute pour les contrôles : elle ne sert qu'aux libellés
//! courts — un verbe, un chiffre —, jamais à ce qui se lit.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub const ATKINSON: &str = "atkinson";
pub const OPENDYSLEXIC: &str = "opendyslexic";
pub const DEFAULT: &str = "default";

/// La police des contrôles : Pixelify Sans, pour les libellés courts.
///
/// Le fichier embarqué est modifié d'un détail : sa ligature « fi » — un
/// glyphe qui se lit « A » à cette taille — est désactivée en renommant
/// l'étiquette OpenType `liga`. La licence OFL de cette police ne réserve pas
/// son nom ; la modification est notée dans `assets/fonts/`.
const PIXEL: &str = "pixel";

struct Face {
    name: &'static str,
    data: &'static [u8],
}

const FACES: &[Face] = &[
    Face {
        name: "atkinson",
        data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Regular.ttf"),
    },
    Face {
        name: "atkinson-bold",
        data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Bold.ttf"),
    },
    Face {
        name: "atkinson-italic",
        data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Italic.ttf"),
    },
    Face {
        name: "atkinson-bold-italic",
        data: include_bytes!("../../assets/fonts/AtkinsonHyperlegible-BoldItalic.ttf"),
    },
    Face {
        name: "opendyslexic",
        data: include_bytes!("../../assets/fonts/OpenDyslexic-Regular.otf"),
    },
    Face {
        name: "opendyslexic-bold",
        data: include_bytes!("../../assets/fonts/OpenDyslexic-Bold.otf"),
    },
    Face {
        name: "opendyslexic-italic",
        data: include_bytes!("../../assets/fonts/OpenDyslexic-Italic.otf"),
    },
    Face {
        name: "opendyslexic-bold-italic",
        data: include_bytes!("../../assets/fonts/OpenDyslexic-BoldItalic.otf"),
    },
    Face { name: PIXEL, data: include_bytes!("../../assets/fonts/PixelifySans.ttf") },
];

/// Installe les polices embarquées et fait de `font` celle de toute la fenêtre.
pub fn install(ctx: &egui::Context, font: &str) {
    let mut definitions = FontDefinitions::default();
    let fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for face in FACES {
        definitions
            .font_data
            .insert(face.name.to_string(), Arc::new(FontData::from_static(face.data)));
        let mut chain = vec![face.name.to_string()];
        chain.extend(fallback.iter().cloned());
        definitions
            .families
            .insert(FontFamily::Name(face.name.into()), chain);
    }
    // L'interface entière parle avec la police de lecture : boutons, listes,
    // réglages. Une seule police à l'écran, c'est une chose de moins à lire.
    if let FontFamily::Name(name) = family(font, false, false) {
        let mut chain = vec![name.to_string()];
        chain.extend(fallback.iter().cloned());
        definitions.families.insert(FontFamily::Proportional, chain);
    }
    ctx.set_fonts(definitions);
}

/// La famille d'un texte de cette police, dans cette graisse et cette posture.
pub fn family(font: &str, strong: bool, italic: bool) -> FontFamily {
    let base = match font {
        ATKINSON => "atkinson",
        OPENDYSLEXIC => "opendyslexic",
        _ => return FontFamily::Proportional,
    };
    let name = match (strong, italic) {
        (false, false) => base.to_string(),
        (true, false) => format!("{base}-bold"),
        (false, true) => format!("{base}-italic"),
        (true, true) => format!("{base}-bold-italic"),
    };
    FontFamily::Name(name.into())
}

/// La police pixel des contrôles, à cette taille.
pub fn pixel(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(PIXEL.into()))
}

/// Vrai quand la police a une vraie italique : egui n'a alors pas à en
/// fabriquer une en penchant les glyphes.
pub fn has_italic(font: &str) -> bool {
    matches!(font, ATKINSON | OPENDYSLEXIC)
}


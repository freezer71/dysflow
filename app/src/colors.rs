//! Couleurs provisoires de l'app, en un seul endroit : le thème et ses
//! couleurs définitives arrivent avec les réglages (tâche 9).
//!
//! Le papier chaud et l'encre presque noire de la vue de lecture ; la barre
//! et l'en-tête posés sur une surface un ton plus sombre ; une seule couleur
//! d'accent, celle du mot lu, que reprennent le disque de lecture et la part
//! lue de la frise.

use gpui::{rgb, rgba, Hsla};

/// Le fond de la page.
pub fn background() -> Hsla {
    rgb(0xfbf8f1).into()
}
/// Le texte lu.
pub fn text() -> Hsla {
    rgb(0x1f1d1a).into()
}
pub fn link() -> Hsla {
    rgb(0x1d4f91).into()
}
/// Le fond arrondi sous le mot lu.
pub fn word() -> Hsla {
    rgba(0xf2c94c99).into()
}
/// La phrase lue quand aucun mot ne l'est.
pub fn sentence() -> Hsla {
    rgba(0xf2c94c40).into()
}
/// La règle sous la ligne lue.
pub fn ruler() -> Hsla {
    rgba(0x8ab4f833).into()
}

/// Les libellés de l'interface (en-tête, barre) : moins forts que le texte
/// lu, toujours au-dessus de 4,5:1 sur la surface.
pub fn chrome() -> Hsla {
    rgb(0x4a463f).into()
}
/// Le texte secondaire (temps, barre d'état) : 5,44:1 sur le papier.
pub fn muted() -> Hsla {
    rgb(0x6b655b).into()
}
/// La surface de la barre de lecture, posée sur la page.
pub fn surface() -> Hsla {
    rgb(0xf1ece1).into()
}
/// Le filet entre la page et l'en-tête ou la barre d'état.
pub fn border() -> Hsla {
    rgb(0xe4ddcf).into()
}
/// L'accent : le disque de lecture et la part lue de la frise.
pub fn accent() -> Hsla {
    rgb(0xb7791f).into()
}
/// Le pictogramme posé sur l'accent.
pub fn on_accent() -> Hsla {
    rgb(0xffffff).into()
}
/// Une erreur dans la barre d'état.
pub fn error() -> Hsla {
    rgb(0xa1341f).into()
}
/// La page sous un fichier qu'on dépose.
pub fn drop_target() -> Hsla {
    rgba(0xb7791f1f).into()
}
/// Le voile sous l'aide.
pub fn overlay() -> Hsla {
    rgba(0x1f1d1a33).into()
}
/// Le fond d'une touche dans l'aide.
pub fn keycap() -> Hsla {
    rgba(0x1f1d1a12).into()
}
/// Un filet plus marqué : la sélection au clavier sur une ligne désactivée
/// du sommaire.
pub fn border_strong() -> Hsla {
    rgb(0xb9b09f).into()
}
/// Un libellé désactivé (titre sans phrase au sommaire) : lisible, mais
/// nettement en retrait du texte actif.
pub fn disabled() -> Hsla {
    rgb(0x9a9387).into()
}
/// Le fond du texte en cours de modification : la page teintée de l'encre,
/// on voit qu'on écrit.
pub fn editing() -> Hsla {
    rgba(0x1f1d1a0d).into()
}

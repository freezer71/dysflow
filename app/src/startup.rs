//! Le démarrage réel de l'app : l'installation automatique de Kokoro au
//! premier lancement (et à chaque « Réessayer »), ce que la barre d'état en
//! dit, la voix de repli qui lit en attendant, et le message quand aucune
//! fenêtre ne peut s'ouvrir (pas de GPU, pas d'écran).
//!
//! Rien ici ne bloque le fil de l'interface : `first_launch` pose la
//! question au fil du lecteur (`Session::post`) et rend sa réponse, qu'on
//! attend sur l'exécuteur d'arrière-plan (`MainWindow::watch_voice_setup`).
//! Le téléchargement lui-même court sur son propre fil, dans le cœur ;
//! l'avancement arrive par l'instantané (`Snapshot.download`).

use std::io::Write;
use std::time::Instant;

use dysflow::i18n::{with_ui_lang, UiLang};
use dysflow::reader::settings::DownloadView;
use dysflow::reader::view::{Snapshot, VoiceState};
use dysflow::tr;

use crate::session::{Reply, Session};

/// Demande au cœur d'installer la voix par défaut (`Reader::ensure_default_voice`) :
/// le Kokoro recommandé se télécharge s'il n'y en a aucun, et devient la voix
/// à la fin ; jusque-là la voix de repli lit. Idempotente (un téléchargement
/// qui court ne se double pas, un échec se relance). Ne bloque pas : la
/// réponse (`Err` : le refus, par exemple la place sur le disque) s'attend
/// hors du fil de l'interface.
pub fn first_launch(session: &Session) -> Reply<Result<(), String>> {
    session.post(|reader| {
        reader
            .ensure_default_voice(Instant::now())
            .map_err(|e| e.to_string())
    })
}

/// Ce que la barre d'état dit de la voix, seulement quand elle n'est pas
/// prête. Dans l'ordre de l'app Mac (`VoiceStatus.swift`) : un échec
/// d'abord (il attend « Réessayer »), puis le téléchargement (la voix de
/// repli lit pendant ce temps : pas de repli à dire en plus), la voix
/// muette, le repli, la première préparation.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceStatus {
    Downloading { received: u64, total: u64 },
    Failed(String),
    Unavailable(String),
    Fallback(String),
    Preparing,
}

impl VoiceStatus {
    /// `setup_error` : le refus rendu par `first_launch`.
    pub fn current(s: &Snapshot, setup_error: Option<&str>) -> Option<VoiceStatus> {
        if let Some(why) = s.download.as_ref().and_then(|d| d.failed.as_ref()) {
            return Some(VoiceStatus::Failed(why.clone()));
        }
        if let Some(why) = setup_error {
            return Some(VoiceStatus::Failed(why.to_string()));
        }
        if let Some(d) = &s.download {
            return Some(VoiceStatus::Downloading {
                received: d.received,
                total: d.total,
            });
        }
        match &s.voice {
            VoiceState::Unavailable { reason } => Some(VoiceStatus::Unavailable(reason.clone())),
            VoiceState::Fallback { reason } => Some(VoiceStatus::Fallback(reason.clone())),
            VoiceState::Loading => Some(VoiceStatus::Preparing),
            VoiceState::Resting | VoiceState::Ready => None,
        }
    }

    /// Le texte entier, « Réessayer » compris quand on peut réessayer.
    pub fn text(&self) -> String {
        if self.can_retry() {
            format!("{} — {}", self.message(), retry_label())
        } else {
            self.message()
        }
    }

    /// Le texte sans « Réessayer » : la barre d'état en fait un bouton.
    pub fn message(&self) -> String {
        match self {
            VoiceStatus::Downloading { received, total } => tr!(
                en = "Installing the voice: {}% of {} — the system voice reads in the meantime",
                fr = "Installation de la voix : {} % de {} — la voix du système lit en attendant",
                percent(*received, *total),
                megabytes(*total)
            ),
            VoiceStatus::Failed(why) => tr!(
                en = "The voice could not be installed: {}",
                fr = "La voix n'a pas pu s'installer : {}",
                why
            ),
            VoiceStatus::Unavailable(why) => tr!(
                en = "Voice unavailable: {}",
                fr = "Voix indisponible : {}",
                why
            ),
            VoiceStatus::Fallback(why) => tr!(
                en = "System voice as a fallback: {}",
                fr = "Voix du système en repli : {}",
                why
            ),
            VoiceStatus::Preparing => {
                tr!("Preparing the voice…", "Préparation de la voix…").to_string()
            }
        }
    }

    pub fn can_retry(&self) -> bool {
        matches!(self, VoiceStatus::Failed(_))
    }

    /// L'avancement du téléchargement, entre 0 et 1.
    pub fn fraction(&self) -> Option<f32> {
        match self {
            VoiceStatus::Downloading { received, total } => {
                Some(percent(*received, *total) as f32 / 100.0)
            }
            _ => None,
        }
    }
}

/// Le libellé du bouton qui relance l'installation.
pub fn retry_label() -> String {
    tr!("Retry", "Réessayer").to_string()
}

/// La ligne d'un téléchargement de voix : « 25 % » en cours, le message et
/// « Réessayer » en échec.
pub fn download_label(d: &DownloadView) -> String {
    match &d.failed {
        Some(why) => VoiceStatus::Failed(why.clone()),
        None => VoiceStatus::Downloading {
            received: d.received,
            total: d.total,
        },
    }
    .text()
}

/// Le pourcentage reçu, arrondi vers le bas, 100 au plus ; 0 si la taille
/// est inconnue.
fn percent(received: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    (received.min(total) as u128 * 100 / total as u128) as u64
}

fn megabytes(bytes: u64) -> String {
    tr!(en = "{:.0} MB", fr = "{:.0} Mo", bytes as f64 / 1_000_000.0)
}

/// Le message quand la fenêtre ne s'ouvre pas (pas de GPU utilisable, pas
/// d'écran) : `err` est l'erreur de GPUI. Toujours dans les deux langues,
/// celle de l'interface d'abord : la fenêtre n'existe pas, rien d'autre ne
/// le dira, et la langue choisie n'est peut-être pas celle de qui lit le
/// terminal.
pub fn no_gpu_message(err: &str) -> String {
    let say = |lang| {
        with_ui_lang(lang, || {
            tr!(
                en = "Dysflow could not open its window: no usable graphics card (GPU) or display was found ({}).",
                fr = "Dysflow n'a pas pu ouvrir sa fenêtre : aucune carte graphique (GPU) ni aucun écran utilisable n'a été trouvé ({}).",
                err
            )
        })
    };
    let (first, second) = match dysflow::i18n::ui_lang() {
        UiLang::Fr => (UiLang::Fr, UiLang::En),
        UiLang::En => (UiLang::En, UiLang::Fr),
    };
    format!("{}\n{}", say(first), say(second))
}

/// La sortie quand aucune fenêtre ne s'ouvre : `no_gpu_message` sur `out`
/// (la sortie d'erreur dans `run`), et le code de sortie, 1.
pub fn no_window(err: &str, out: &mut impl Write) -> i32 {
    let _ = writeln!(out, "{}", no_gpu_message(err));
    let _ = out.flush();
    1
}

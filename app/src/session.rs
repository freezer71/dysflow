//! Le lien entre l'interface et le lecteur, qui tourne sur son propre fil.

use std::sync::Arc;

use dysflow::reader::handle::{ReaderHandle, Request};
use dysflow::reader::settings::SettingsView;
use dysflow::reader::view::{DocView, Snapshot};

/// Le lecteur vu de l'interface : on lui envoie des requêtes, on lit ce
/// qu'il publie. Rien ici ne bloque, sauf `settings`.
pub struct Session {
    handle: ReaderHandle,
}

impl Session {
    pub fn new(handle: ReaderHandle) -> Self {
        Session { handle }
    }

    pub fn send(&self, r: Request) {
        self.handle.send(r);
    }

    /// Le dernier instantané publié par le fil du lecteur.
    pub fn snapshot(&self) -> Snapshot {
        self.handle.snapshot()
    }

    /// Le document de la dernière révision publiée.
    pub fn document(&self) -> Arc<DocView> {
        self.handle.document()
    }

    /// Les réglages du lecteur. BLOQUE jusqu'à la réponse du fil du lecteur
    /// (5 s au plus) : jamais depuis le fil de l'interface, mais sur
    /// l'exécuteur d'arrière-plan (voir `ReadingView::reload_settings`).
    /// `None` si le fil ne répond pas.
    pub fn settings(&self) -> Option<SettingsView> {
        self.handle.ask(|reader| reader.settings())
    }
}

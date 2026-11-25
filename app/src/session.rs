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

    /// Exécute `f` sur le fil du lecteur et rend son résultat. BLOQUE comme
    /// `settings` (5 s au plus, puis `None`) : jamais depuis le fil de
    /// l'interface.
    pub fn ask<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut dysflow::reader::Reader) -> T + Send + 'static,
    ) -> Option<T> {
        self.handle.ask(f)
    }

    /// Envoie `f` au fil du lecteur sans attendre : la requête part tout de
    /// suite, dans l'ordre des appels (deux réglages changés de suite
    /// s'appliquent dans cet ordre). Ne bloque pas : la réponse s'attend par
    /// `Reply::wait`, sur l'exécuteur d'arrière-plan.
    pub fn post<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut dysflow::reader::Reader) -> T + Send + 'static,
    ) -> Reply<T> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.handle.send(Request::Ask(Box::new(move |reader| {
            let _ = tx.send(f(reader));
        })));
        Reply(rx)
    }
}

/// La réponse à une requête de `Session::post`.
pub struct Reply<T>(std::sync::mpsc::Receiver<T>);

impl<T> Reply<T> {
    /// BLOQUE jusqu'à la réponse (5 s au plus, puis `None`) : jamais depuis
    /// le fil de l'interface.
    pub fn wait(self) -> Option<T> {
        self.0.recv_timeout(std::time::Duration::from_secs(5)).ok()
    }

    /// Attend la réponse aussi longtemps qu'il faut : la requête est dans la
    /// file du lecteur et s'appliquera, même s'il est occupé longtemps.
    /// `None` seulement si le lecteur s'est arrêté. BLOQUE : jamais depuis le
    /// fil de l'interface.
    pub fn wait_unbounded(self) -> Option<T> {
        self.0.recv().ok()
    }
}

import AppKit

/// Que montrer en arrivant : rien, le lecteur vide (toujours en grand), ou
/// la dernière taille utilisée.
enum LaunchPresentation: Equatable {
    case nothing, emptyReader, lastSize
}

/// Ouvert par l'utilisateur, par macOS à l'ouverture de session, ou pour
/// ouvrir des fichiers ?
enum LaunchKind {
    /// Lu dans l'événement Apple du lancement (à appeler pendant
    /// `applicationWillFinishLaunching`). Sans lui, un démarrage dans les
    /// deux premières minutes de la machine passe pour une ouverture de
    /// session : rien ne s'impose au démarrage.
    @MainActor
    static func launchedAtLogin(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent,
                                uptime: TimeInterval = ProcessInfo.processInfo.systemUptime) -> Bool {
        if let event, event.eventID == kAEOpenApplication,
           event.paramDescriptor(forKeyword: keyAEPropData)?.enumCodeValue == keyAELaunchedAsLogInItem {
            return true
        }
        return uptime < 120
    }

    /// Lancé pour ouvrir des fichiers (`sillage fichier.pdf`, « Ouvrir
    /// avec ») : le lecteur les montre à leur arrivée, par
    /// `application(_:open:)`, parfois après la fin du lancement.
    @MainActor
    static func launchedWithFiles(event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent) -> Bool {
        event?.eventID == kAEOpenDocuments
    }

    /// Au lancement : rien à la connexion ni avec des fichiers ; à la main,
    /// le lecteur.
    static func presentation(atLogin: Bool, withFiles: Bool, hasDocument: Bool) -> LaunchPresentation {
        if atLogin || withFiles { return .nothing }
        return reopenPresentation(hasDocument: hasDocument)
    }

    /// Rouvert (Dock, Finder, Spotlight, lanceur) : le lecteur vide en
    /// grand, ou la dernière taille s'il y a un document.
    static func reopenPresentation(hasDocument: Bool) -> LaunchPresentation {
        hasDocument ? .lastSize : .emptyReader
    }
}

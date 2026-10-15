/// Les deux formes de sillon, et « masqué ». Une seule fenêtre : le
/// lecteur (texte, en-tête, barre) ou le mini (une barre flottante).
enum DisplayMode: String, Equatable, CaseIterable {
    case mini, reader, hidden

    var isVisible: Bool { self != .hidden }

    /// Une valeur retenue, y compris celles de la version précédente
    /// (petit, moyen, grand, arrière-plan) ; nil si elle est illisible.
    static func stored(_ raw: String) -> DisplayMode? {
        if let mode = DisplayMode(rawValue: raw) { return mode }
        switch raw {
        case "small": return .mini
        case "medium", "large": return .reader
        case "background": return .hidden
        default: return nil
        }
    }
}

/// Ce qui fait changer de forme.
enum DisplayEvent: Equatable {
    /// Une forme précise : le lecteur vide au lancement manuel, l'édition,
    /// la recherche ou l'aide demandées depuis le mini.
    case choose(DisplayMode)
    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière
    /// taille ; une forme visible ne change pas.
    case show
    /// ⌃⌥S : la dernière taille, ou masqué.
    case toggleVisibility
    /// ⌘M, « Réduire », « Agrandir » : du lecteur au mini et retour.
    case toggleSize
    /// ⌘W, Échap, le bouton fermer : masqué (jamais le mini).
    case close
}

/// La forme courante et la dernière forme visible — jamais « masqué », pour
/// que toute ouverture montre quelque chose. Sans état caché : testé seul.
struct DisplayState: Equatable {
    private(set) var current: DisplayMode
    private(set) var lastVisible: DisplayMode

    static let initial = DisplayState(current: .hidden, lastVisible: .reader)

    init(current: DisplayMode, lastVisible: DisplayMode) {
        self.current = current
        if current.isVisible {
            self.lastVisible = current
        } else {
            // Une dernière forme « masqué » (préférence abîmée) : le lecteur.
            self.lastVisible = lastVisible.isVisible ? lastVisible : .reader
        }
    }

    func applying(_ event: DisplayEvent) -> DisplayState {
        switch event {
        case .choose(let mode):
            return DisplayState(current: mode, lastVisible: lastVisible)
        case .show:
            return current.isVisible ? self : DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .toggleVisibility:
            return current.isVisible ? DisplayState(current: .hidden, lastVisible: current)
                                     : DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .toggleSize:
            switch current {
            case .mini: return DisplayState(current: .reader, lastVisible: .reader)
            case .reader: return DisplayState(current: .mini, lastVisible: .mini)
            case .hidden: return self
            }
        case .close:
            return DisplayState(current: .hidden, lastVisible: current.isVisible ? current : lastVisible)
        }
    }

    /// La fenêtre disparaît : la lecture s'arrête.
    func hides(_ next: DisplayState) -> Bool {
        current.isVisible && !next.current.isVisible
    }

    /// Après un lancement, rien n'est montré ; la dernière taille reste.
    var relaunched: DisplayState {
        DisplayState(current: .hidden, lastVisible: lastVisible)
    }
}

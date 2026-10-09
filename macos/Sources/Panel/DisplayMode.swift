/// Les quatre façons de montrer sillon. Une seule fenêtre : chaque mode
/// change son contenu et sa taille ; l'arrière-plan la retire, la lecture
/// continue.
enum DisplayMode: String, Equatable, CaseIterable {
    case small, medium, large, background

    var isVisible: Bool { self != .background }
}

/// Ce qui fait changer de mode.
enum DisplayEvent: Equatable {
    /// ⌘1, ⌘2, ⌘3, ⏎, le sous-menu « Taille ».
    case choose(DisplayMode)
    /// Échap : du grand au moyen, sinon l'arrière-plan.
    case escape
    /// ⌘W, fermeture de la fenêtre : l'arrière-plan d'un coup.
    case hide
    /// ⌃⌥S, « Afficher sillon » : le dernier mode visible.
    case showLast
    /// ⌃⌥Espace : le mode en cours ; depuis l'arrière-plan, le moyen.
    case readSelection
}

/// Le mode courant et le dernier mode visible — jamais l'arrière-plan, pour
/// que ⌃⌥S montre toujours quelque chose. Sans état caché : testé seul.
struct DisplayState: Equatable {
    private(set) var current: DisplayMode
    private(set) var lastVisible: DisplayMode

    static let initial = DisplayState(current: .background, lastVisible: .medium)

    init(current: DisplayMode, lastVisible: DisplayMode) {
        self.current = current
        if current.isVisible {
            self.lastVisible = current
        } else {
            // Un dernier mode « arrière-plan » (préférence abîmée) : le moyen.
            self.lastVisible = lastVisible.isVisible ? lastVisible : .medium
        }
    }

    func applying(_ event: DisplayEvent) -> DisplayState {
        switch event {
        case .choose(let mode):
            return DisplayState(current: mode, lastVisible: lastVisible)
        case .escape:
            let next: DisplayMode = current == .large ? .medium : .background
            return DisplayState(current: next, lastVisible: lastVisible)
        case .hide:
            return DisplayState(current: .background, lastVisible: lastVisible)
        case .showLast:
            return DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .readSelection:
            return current.isVisible ? self : DisplayState(current: .medium, lastVisible: lastVisible)
        }
    }
}

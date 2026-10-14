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

    /// `playing` : la voix lit. Fermer le moyen ou le grand ne retire alors
    /// pas la fenêtre : la barre du mode petit reste, pour garder la main sur
    /// le son. Depuis la barre, la fermeture la retire, lecture comprise.
    func applying(_ event: DisplayEvent, playing: Bool = false) -> DisplayState {
        switch event {
        case .choose(let mode):
            return DisplayState(current: mode, lastVisible: lastVisible)
        case .escape:
            if current == .large { return DisplayState(current: .medium, lastVisible: lastVisible) }
            return closing(playing: playing)
        case .hide:
            return closing(playing: playing)
        case .showLast:
            return DisplayState(current: lastVisible, lastVisible: lastVisible)
        case .readSelection:
            return current.isVisible ? self : DisplayState(current: .medium, lastVisible: lastVisible)
        }
    }

    /// La fenêtre se ferme : l'arrière-plan, sauf pendant la lecture depuis
    /// le moyen ou le grand, où la barre prend sa place. « Afficher sillon »
    /// rendra ensuite le mode fermé, pas la barre.
    private func closing(playing: Bool) -> DisplayState {
        guard playing, current == .medium || current == .large else {
            return DisplayState(current: .background, lastVisible: lastVisible)
        }
        var next = DisplayState(current: .small, lastVisible: lastVisible)
        next.lastVisible = current
        return next
    }
}

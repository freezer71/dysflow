import AppKit

/// Le clavier du panneau : un moniteur local des touches, posé tant que la
/// fenêtre est montrée. Il prend les touches de lecture et laisse au champ de
/// saisie ou à la liste qui a le focus celles qui leur reviennent.
@MainActor
final class PanelKeyRouter {
    /// Ce que devient une touche.
    enum Route: Equatable {
        /// Laissée à la fenêtre (champ de saisie, liste, touche sans action).
        case pass
        /// ⌘↩ en édition : la modification s'applique.
        case finishEditing
        /// Une action du lecteur.
        case perform(ReaderAction)
    }

    private let window: NSWindow
    private let isEditing: () -> Bool
    private let action: (KeyPress) -> ReaderAction?
    private let finishEditing: () -> Void
    private let perform: (ReaderAction) -> Void
    private var monitor: Any?

    init(window: NSWindow,
         isEditing: @escaping () -> Bool,
         action: @escaping (KeyPress) -> ReaderAction?,
         finishEditing: @escaping () -> Void,
         perform: @escaping (ReaderAction) -> Void) {
        self.window = window
        self.isEditing = isEditing
        self.action = action
        self.finishEditing = finishEditing
        self.perform = perform
    }

    /// La décision, sans événement ni fenêtre : testable.
    static func route(keyCode: UInt16, command: Bool, option: Bool, editing: Bool,
                      firstResponder: NSResponder?, action: () -> ReaderAction?) -> Route {
        // En édition, ⌘↩ valide comme Échap.
        if editing, keyCode == KeyCode.enter, command {
            return .finishEditing
        }
        // Un champ de saisie garde ses touches, sauf Échap.
        if firstResponder is NSTextView, !(firstResponder is ReadingTextView), keyCode != KeyCode.escape {
            return .pass
        }
        // Une liste (sommaire, signets) garde ses flèches verticales et
        // Entrée : on la parcourt, puis on y va.
        if firstResponder is NSTableView, [KeyCode.up, KeyCode.down, KeyCode.enter].contains(keyCode), !option {
            return .pass
        }
        guard let action = action() else { return .pass }
        return .perform(action)
    }

    func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            let route = Self.route(keyCode: event.keyCode,
                                   command: event.modifierFlags.contains(.command),
                                   option: event.modifierFlags.contains(.option),
                                   editing: self.isEditing(),
                                   firstResponder: self.window.firstResponder,
                                   action: { self.action(KeyPress(event)) })
            switch route {
            case .pass:
                return event
            case .finishEditing:
                self.finishEditing()
                return nil
            case .perform(let action):
                self.perform(action)
                return nil
            }
        }
    }

    func remove() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
}

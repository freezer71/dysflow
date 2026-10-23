import AppKit

/// Codes matériels des touches sans caractère.
enum KeyCode {
    static let left: UInt16 = 123, right: UInt16 = 124, down: UInt16 = 125, up: UInt16 = 126
    static let enter: UInt16 = 36, escape: UInt16 = 53, space: UInt16 = 49
    static let home: UInt16 = 115
}

struct KeyPress: Equatable {
    var characters: String
    var keyCode: UInt16
    var option = false
    var shift = false
    var command = false
    var control = false

    init(characters: String, keyCode: UInt16 = 0, option: Bool = false,
         shift: Bool = false, command: Bool = false, control: Bool = false) {
        self.characters = characters
        self.keyCode = keyCode
        self.option = option
        self.shift = shift
        self.command = command
        self.control = control
    }

    init(_ event: NSEvent) {
        let flags = event.modifierFlags
        self.init(characters: event.charactersIgnoringModifiers ?? "",
                  keyCode: event.keyCode,
                  option: flags.contains(.option),
                  shift: flags.contains(.shift),
                  command: flags.contains(.command),
                  control: flags.contains(.control))
    }
}

enum ReaderAction: Equatable {
    case command(ReaderCommand)
    /// Une ligne affichée plus haut ou plus bas : seule la vue sait où elle
    /// commence.
    case line(delta: Int)
    /// Échap : la saisie se valide, l'aide, la recherche ou le sommaire se
    /// ferment, sinon le lecteur (comme ⌘W).
    case escape
    /// ⌘W : fermer (la lecture s'arrête).
    case close
    /// ⌘M : du lecteur au mini et retour.
    case toggleSize
    /// Le bouton « Réduire dans le Dock » : la fenêtre part dans le Dock,
    /// la lecture continue.
    case minimize
    /// ⌘C : la sélection, sinon la phrase en cours.
    case copy
    /// ⌘V : lire le presse-papiers.
    case paste
    /// ⌘O : le panneau d'ouverture.
    case openFile
    case openSearch, toggleSidebar, showHelp, openSettings
    /// Le texte devient modifiable (bouton Modifier, ou « e »).
    case edit
}

/// Les touches du lecteur : la table de la spec (1.5), rien d'autre.
enum KeyMap {
    static func action(for key: KeyPress) -> ReaderAction? {
        if key.command {
            if key.option || key.control { return nil }
            switch key.characters {
            case "c": return .copy
            case "v": return .paste
            case "o": return .openFile
            case "f": return .openSearch
            case "m": return .toggleSize
            case "w": return .close
            case ",": return .openSettings
            default: return nil
            }
        }
        if key.option || key.control { return nil }

        // Les touches sans caractère.
        switch key.keyCode {
        case KeyCode.space: return .command(.togglePlay)
        case KeyCode.escape: return .escape
        case KeyCode.home: return .command(.start)
        case KeyCode.left, KeyCode.right:
            guard !key.shift else { return nil }
            return .command(.sentences(delta: key.keyCode == KeyCode.left ? -1 : 1))
        case KeyCode.up, KeyCode.down:
            guard !key.shift else { return nil }
            return .line(delta: key.keyCode == KeyCode.up ? -1 : 1)
        default: break
        }

        switch key.characters {
        case "n": return .command(.sections(delta: 1))
        case "p": return .command(.sections(delta: -1))
        case "N": return .command(.searchAdvance(delta: 1))
        case "P": return .command(.searchAdvance(delta: -1))
        case "+": return .command(.changeSpeed(delta: 0.1))
        case "-": return .command(.changeSpeed(delta: -0.1))
        case "/": return .openSearch
        case "t": return .toggleSidebar
        case "e": return .edit
        case "?": return .showHelp
        default: return nil
        }
    }
}

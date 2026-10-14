import AppKit

/// Codes matériels des touches sans caractère.
enum KeyCode {
    static let left: UInt16 = 123, right: UInt16 = 124, down: UInt16 = 125, up: UInt16 = 126
    static let enter: UInt16 = 36, escape: UInt16 = 53, space: UInt16 = 49
    static let home: UInt16 = 115, end: UInt16 = 119
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
    case expand, collapseOrHide, copySentence, openSearch
    /// ⌘1, ⌘2, ⌘3 : petit, moyen, grand.
    case display(DisplayMode)
    case toggleSidebar, toggleBookmarks, showHelp, openSettings, openVoiceSettings
    /// Le texte devient modifiable (double clic, ou « e »).
    case edit
}

/// Les touches du terminal, plus les gestes propres à la fenêtre.
enum KeyMap {
    static func action(for key: KeyPress, seekStep: Int32 = 5) -> ReaderAction? {
        if key.command {
            switch key.characters {
            case "c": return .copySentence
            case ",": return .openSettings
            // QWERTY et AZERTY non décalé : les mêmes touches physiques.
            case "1", "&": return .display(.small)
            case "2", "é": return .display(.medium)
            case "3", "\"": return .display(.large)
            default: return nil
            }
        }

        // Les touches sans caractère : flèches, entrée, etc.
        switch key.keyCode {
        case KeyCode.space: return .command(.togglePlay)
        case KeyCode.enter: return .expand
        case KeyCode.escape: return .collapseOrHide
        case KeyCode.home: return .command(.start)
        case KeyCode.end: return .command(.end)
        case KeyCode.left, KeyCode.right:
            let sign: Int32 = key.keyCode == KeyCode.left ? -1 : 1
            if key.option { return .command(.words(delta: sign)) }
            if key.shift { return .command(.sentences(delta: sign * seekStep)) }
            return .command(.sentences(delta: sign))
        case KeyCode.up, KeyCode.down:
            // ⌥ volume, ⇧ bloc, sinon ligne affichée
            let sign: Int32 = key.keyCode == KeyCode.up ? -1 : 1
            if key.option { return .command(.changeVolume(delta: -0.05 * Float(sign))) }
            if key.shift { return .command(.blocks(delta: sign)) }
            return .line(delta: Int(sign))
        default: break
        }

        // Les raccourcis caractères : bloqués si Option ou Control
        if key.option || key.control { return nil }

        switch key.characters {
        case "n": return .command(.sections(delta: 1))
        case "p": return .command(.sections(delta: -1))
        case "N": return .command(.searchAdvance(delta: 1))
        case "P": return .command(.searchAdvance(delta: -1))
        case "+", "=": return .command(.changeSpeed(delta: 0.1))
        case "-", "_": return .command(.changeSpeed(delta: -0.1))
        // QWERTY et accents French AZERTY non décalés
        case "1", "&": return .command(.setSpeed(speed: 0.75))
        case "2", "é": return .command(.setSpeed(speed: 1.0))
        case "3", "\"": return .command(.setSpeed(speed: 1.5))
        case "4", "'": return .command(.setSpeed(speed: 2.0))
        case "[": return .command(.changeVolume(delta: -0.05))
        case "]": return .command(.changeVolume(delta: 0.05))
        case "m": return .command(.toggleMute)
        case "r": return .command(.restartSentence)
        case "b": return .command(.toggleBookmark)
        case "g": return .command(.start)
        case "B": return .toggleBookmarks
        case "t": return .toggleSidebar
        case "/": return .openSearch
        case "?", "h": return .showHelp
        case "v": return .openVoiceSettings
        case "e": return .edit
        default: return nil
        }
    }
}

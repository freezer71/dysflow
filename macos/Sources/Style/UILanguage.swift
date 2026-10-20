import Foundation

/// La langue de l'interface, lue à un seul endroit : celle que macOS a choisie
/// parmi les langues de l'app (`en`, `fr`). Les nombres, les unités et les
/// dates s'écrivent à la française en français, à l'anglaise sinon.
enum UILanguage {
    /// Le code donné au moteur Rust (`setInterfaceLanguage`).
    static var tag: String { Bundle.main.preferredLocalizations.first ?? "en" }

    static var isFrench: Bool { tag == "fr" }

    /// Les réglages régionaux des nombres et des dates affichés.
    static var locale: Locale { Locale(identifier: isFrench ? "fr_FR" : "en_US") }
}

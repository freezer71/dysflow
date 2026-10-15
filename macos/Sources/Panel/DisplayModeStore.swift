import Foundation

/// Retient la forme courante, la dernière forme visible, la place du mini
/// et le cadre du lecteur, d'un lancement à l'autre.
final class DisplayModeStore {
    static let currentKey = "affichage.mode"
    static let lastVisibleKey = "affichage.dernierVisible"
    static let miniOriginKey = "affichage.petit.origine"
    static let readerFrameKey = "affichage.lecteur.cadre"

    private let defaults: UserDefaults

    init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Une valeur illisible retombe sur l'état initial (masqué, lecteur) ;
    /// celles de la version précédente se traduisent (`DisplayMode.stored`).
    var state: DisplayState {
        get {
            let current = defaults.string(forKey: Self.currentKey).flatMap(DisplayMode.stored)
                ?? DisplayState.initial.current
            let last = defaults.string(forKey: Self.lastVisibleKey).flatMap(DisplayMode.stored)
                ?? DisplayState.initial.lastVisible
            return DisplayState(current: current, lastVisible: last)
        }
        set {
            defaults.set(newValue.current.rawValue, forKey: Self.currentKey)
            defaults.set(newValue.lastVisible.rawValue, forKey: Self.lastVisibleKey)
        }
    }

    /// Le coin bas gauche du mini, là où l'utilisateur l'a laissé ; nil
    /// tant qu'il ne l'a pas déplacé.
    var miniOrigin: CGPoint? {
        get {
            guard let xy = numbers(Self.miniOriginKey, count: 2) else { return nil }
            return CGPoint(x: xy[0], y: xy[1])
        }
        set {
            if let p = newValue {
                defaults.set([Double(p.x), Double(p.y)], forKey: Self.miniOriginKey)
            } else {
                defaults.removeObject(forKey: Self.miniOriginKey)
            }
        }
    }

    /// Le cadre du lecteur tel que l'utilisateur l'a laissé ; nil tant
    /// qu'il ne l'a ni déplacé ni redimensionné.
    var readerFrame: CGRect? {
        get {
            guard let f = numbers(Self.readerFrameKey, count: 4) else { return nil }
            return CGRect(x: f[0], y: f[1], width: f[2], height: f[3])
        }
        set {
            if let r = newValue {
                defaults.set([Double(r.minX), Double(r.minY), Double(r.width), Double(r.height)],
                             forKey: Self.readerFrameKey)
            } else {
                defaults.removeObject(forKey: Self.readerFrameKey)
            }
        }
    }

    private func numbers(_ key: String, count: Int) -> [Double]? {
        guard let values = defaults.array(forKey: key) as? [Double], values.count == count,
              values.allSatisfy(\.isFinite) else { return nil }
        return values
    }

    /// Les préférences de l'utilisateur, sauf sous les tests (unitaires ou
    /// d'interface) : une suite à part, vidée à chaque appel.
    static func processDefaults() -> UserDefaults {
        let info = ProcessInfo.processInfo
        let testing = info.arguments.contains("--ui-testing")
            || info.environment["XCTestConfigurationFilePath"] != nil
        guard testing else { return .standard }
        let name = "fr.sillon.tests.affichage"
        guard let suite = UserDefaults(suiteName: name) else { return .standard }
        suite.removePersistentDomain(forName: name)
        return suite
    }
}

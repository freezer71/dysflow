import Foundation

/// Retient le mode courant, le dernier mode visible et la place du mode
/// petit, d'un lancement à l'autre.
final class DisplayModeStore {
    static let currentKey = "affichage.mode"
    static let lastVisibleKey = "affichage.dernierVisible"
    static let smallOriginKey = "affichage.petit.origine"

    private let defaults: UserDefaults

    init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Une valeur illisible retombe sur l'état initial (arrière-plan, moyen).
    var state: DisplayState {
        get {
            let current = defaults.string(forKey: Self.currentKey).flatMap(DisplayMode.init(rawValue:))
                ?? DisplayState.initial.current
            let last = defaults.string(forKey: Self.lastVisibleKey).flatMap(DisplayMode.init(rawValue:))
                ?? DisplayState.initial.lastVisible
            return DisplayState(current: current, lastVisible: last)
        }
        set {
            defaults.set(newValue.current.rawValue, forKey: Self.currentKey)
            defaults.set(newValue.lastVisible.rawValue, forKey: Self.lastVisibleKey)
        }
    }

    /// Le coin bas gauche de la barre du mode petit, là où l'utilisateur l'a
    /// laissée ; nil tant qu'il ne l'a pas déplacée.
    var smallOrigin: CGPoint? {
        get {
            guard let xy = defaults.array(forKey: Self.smallOriginKey) as? [Double], xy.count == 2,
                  xy.allSatisfy(\.isFinite) else { return nil }
            return CGPoint(x: xy[0], y: xy[1])
        }
        set {
            if let p = newValue {
                defaults.set([Double(p.x), Double(p.y)], forKey: Self.smallOriginKey)
            } else {
                defaults.removeObject(forKey: Self.smallOriginKey)
            }
        }
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

/// La barre du mode petit : taille fixe, place retenue, toujours sur un
/// écran visible.
enum SmallPlacement {
    static let size = CGSize(width: 320, height: 76)

    /// La place retenue si un écran en montre au moins un bout (ramenée
    /// entière sur l'écran qui en montre le plus) ; sinon — écran débranché,
    /// rien de retenu — la place par défaut sur l'écran principal.
    static func frame(savedOrigin: CGPoint?, screens: [CGRect], main: CGRect) -> CGRect {
        if let origin = savedOrigin {
            let wanted = CGRect(origin: origin, size: size)
            let best = screens.max { area($0.intersection(wanted)) < area($1.intersection(wanted)) }
            if let screen = best, area(screen.intersection(wanted)) > 0 {
                return clamp(wanted, into: screen)
            }
        }
        return defaultFrame(on: main)
    }

    /// Centrée, le haut au quart de l'écran, comme le panneau.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - size.width / 2, y: top - size.height,
                      width: size.width, height: size.height)
    }

    private static func clamp(_ r: CGRect, into s: CGRect) -> CGRect {
        CGRect(x: min(max(r.minX, s.minX), s.maxX - r.width),
               y: min(max(r.minY, s.minY), s.maxY - r.height),
               width: r.width, height: r.height)
    }

    private static func area(_ r: CGRect) -> CGFloat {
        r.isNull ? 0 : r.width * r.height
    }
}

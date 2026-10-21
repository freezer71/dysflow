import CoreGraphics

/// Un cadre ramené sur un écran visible.
enum ScreenFit {
    /// L'écran qui montre la plus grande part de `wanted`, s'il en montre un bout.
    static func bestScreen(for wanted: CGRect, among screens: [CGRect]) -> CGRect? {
        let best = screens.max { area($0.intersection(wanted)) < area($1.intersection(wanted)) }
        guard let best, area(best.intersection(wanted)) > 0 else { return nil }
        return best
    }

    /// `r` déplacé (pas redimensionné) pour tenir dans `s`.
    static func clamp(_ r: CGRect, into s: CGRect) -> CGRect {
        CGRect(x: min(max(r.minX, s.minX), s.maxX - r.width),
               y: min(max(r.minY, s.minY), s.maxY - r.height),
               width: r.width, height: r.height)
    }

    static func area(_ r: CGRect) -> CGFloat {
        r.isNull ? 0 : r.width * r.height
    }
}

/// Le mini : taille fixe, place retenue, toujours sur un écran visible.
enum MiniPlacement {
    static let size = CGSize(width: 340, height: 128)

    /// La place retenue si un écran en montre au moins un bout (ramenée
    /// entière sur l'écran qui en montre le plus) ; sinon la place par
    /// défaut sur l'écran principal.
    static func frame(savedOrigin: CGPoint?, screens: [CGRect], main: CGRect) -> CGRect {
        if let origin = savedOrigin {
            let wanted = CGRect(origin: origin, size: size)
            if let screen = ScreenFit.bestScreen(for: wanted, among: screens) {
                return ScreenFit.clamp(wanted, into: screen)
            }
        }
        return defaultFrame(on: main)
    }

    /// Centré, le haut au quart de l'écran.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let top = screen.maxY - screen.height * 0.25
        return CGRect(x: screen.midX - size.width / 2, y: top - size.height,
                      width: size.width, height: size.height)
    }
}

/// Le lecteur : redimensionnable, cadre retenu, toujours entier sur un écran.
enum ReaderPlacement {
    static let minimumSize = CGSize(width: 420, height: 260)

    /// Centré, le haut au cinquième de l'écran, 760 pt de large au plus ;
    /// en points entiers, comme AppKit pose la fenêtre.
    static func defaultFrame(on screen: CGRect) -> CGRect {
        let width = min(PanelGeometry.maximumWidth, screen.width * 0.6).rounded(.down)
        let height = min(560, screen.height * 0.6).rounded(.down)
        let top = (screen.maxY - screen.height * 0.2).rounded(.down)
        return CGRect(x: (screen.midX - width / 2).rounded(.down), y: top - height, width: width, height: height)
    }

    /// Le cadre retenu, ramené sur l'écran qui en montre le plus, à une
    /// taille comprise entre le minimum et cet écran ; sinon le défaut.
    static func frame(saved: CGRect?, screens: [CGRect], main: CGRect) -> CGRect {
        guard let saved, saved.width > 0, saved.height > 0,
              let screen = ScreenFit.bestScreen(for: saved, among: screens) else {
            return defaultFrame(on: main)
        }
        let size = CGSize(width: min(max(saved.width, minimumSize.width), screen.width),
                          height: min(max(saved.height, minimumSize.height), screen.height))
        return ScreenFit.clamp(CGRect(origin: saved.origin, size: size), into: screen)
    }
}

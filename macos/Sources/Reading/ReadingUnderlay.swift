import AppKit
import QuartzCore

/// Sous le texte lu, la bande de la ligne et la pastille du mot, en calques :
/// Core Animation les fait glisser d'un mot à l'autre, hors du fil principal,
/// sans redessiner le texte. Seul le remplissage de la pastille (la part dite
/// du mot) change à chaque image, par sa largeur.
///
/// Posée dans la vue de défilement juste sous la vue du texte, au même
/// cadre : ses coordonnées sont celles du texte, et elle défile avec lui.
final class ReadingUnderlay: NSView {
    private let band = CALayer()
    private let pill = CALayer()
    private let wipe = CALayer()
    /// La pastille visée (la position animée est celle du calque de rendu).
    private var pillTarget: CGRect?

    /// Le glissement d'un mot à l'autre, et d'une ligne à l'autre.
    static let pillDuration: CFTimeInterval = 0.16
    static let bandDuration: CFTimeInterval = 0.24

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .never
        for l in [band, pill] {
            l.isHidden = true
            l.cornerCurve = .continuous
            layer?.addSublayer(l)
        }
        pill.masksToBounds = true
        pill.addSublayer(wipe)
        wipe.anchorPoint = .zero
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) non pris en charge") }

    override var isFlipped: Bool { true }
    // Les clics vont au texte, au-dessus.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func apply(_ theme: Theme) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        band.backgroundColor = theme.wordFill.withAlphaComponent(theme.bandAlpha).cgColor
        pill.backgroundColor = theme.pill.withAlphaComponent(theme.pillAlpha).cgColor
        wipe.backgroundColor = theme.pill.withAlphaComponent(theme.wipeAlpha).cgColor
        wipe.isHidden = theme.wipeAlpha == 0
        CATransaction.commit()
    }

    /// Pose la bande et la pastille ; `animated`, elles y glissent.
    func place(band bandRect: CGRect?, pill pillRect: CGRect?, animated: Bool) {
        pillTarget = pillRect
        move(band, to: bandRect, radius: { min(12, $0.height / 2) },
             duration: animated ? Self.bandDuration : 0)
        move(pill, to: pillRect, radius: { min(9, $0.height / 2) },
             duration: animated ? Self.pillDuration : 0)
    }

    /// La part dite : du bord gauche de la pastille jusqu'à `x` (en
    /// coordonnées du texte) ; nil, rien de rempli.
    func fill(upTo x: CGFloat?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if let x, let target = pillTarget {
            let width = min(max(x - target.minX, 0), target.width)
            wipe.frame = CGRect(x: 0, y: 0, width: width, height: target.height)
        } else {
            wipe.frame = .zero
        }
        CATransaction.commit()
    }

    private func move(_ layer: CALayer, to rect: CGRect?, radius: (CGRect) -> CGFloat,
                      duration: CFTimeInterval) {
        CATransaction.begin()
        guard let rect else {
            CATransaction.setDisableActions(true)
            layer.isHidden = true
            CATransaction.commit()
            return
        }
        // Sans cible précédente (premier mot, retour de pause), on pose.
        let animate = duration > 0 && !layer.isHidden
        if animate {
            CATransaction.setAnimationDuration(duration)
            CATransaction.setAnimationTimingFunction(CAMediaTimingFunction(controlPoints: 0.2, 0.8, 0.2, 1))
        } else {
            CATransaction.setDisableActions(true)
        }
        layer.isHidden = false
        layer.frame = rect
        layer.cornerRadius = radius(rect)
        CATransaction.commit()
    }
}

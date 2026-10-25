import AppKit
import SwiftUI

// Le style de l'app, à la manière du style « Felt » (15 oct. 2026) : des
// gris neutres, des cartes blanches très arrondies posées sur une page grise,
// sans bordure ; des champs et des boutons pleins ; l'action principale à
// l'encre (noir le jour, blanc le soir) ; le vert citron pour ce qui est
// choisi. Les contrôles restent petits : ils ne grandissent jamais avec le
// texte lu.

extension NSColor {
    /// Une couleur qui suit l'apparence : claire sous Aqua, sombre sous Dark Aqua.
    static func adaptive(light: UInt32, dark: UInt32) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
                ? NSColor(hex: dark) : NSColor(hex: light)
        }
    }
}

/// Les couleurs de l'interface (fenêtres, réglages). Le panneau de lecture a
/// les siennes (`Theme`), accordées à celles-ci.
enum Palette {
    static let page = Color(nsColor: .adaptive(light: 0xF2F2F2, dark: 0x0E0E0E))
    /// Le fond des cartes, posées sur la page.
    static let sidebar = Color(nsColor: .adaptive(light: 0xFFFFFF, dark: 0x1C1C1C))
    /// Le texte courant : 15:1 au moins sur les cartes.
    static let ink = Color(nsColor: .adaptive(light: 0x141414, dark: 0xEDEDED))
    /// Descriptions et légendes : 5:1 au moins sur les cartes et la page (WCAG AA).
    static let muted = Color(nsColor: .adaptive(light: 0x666666, dark: 0xA0A0A0))
    /// Les icônes seulement, jamais du texte.
    static let faint = Color(nsColor: .adaptive(light: 0x8F8F8F, dark: 0x7A7A7A))
    static let divider = Color(nsColor: .adaptive(light: 0xEDEDED, dark: 0x2A2A2A))
    static let hover = Color(nsColor: .adaptive(light: 0xE9E9E9, dark: 0x2C2C2C))
    static let selected = Color(nsColor: .adaptive(light: 0xE0E0E0, dark: 0x343434))
    /// Le fond d'un encadré (« callout »).
    static let callout = Color(nsColor: .adaptive(light: 0xFFFFFF, dark: 0x1C1C1C))
    /// Le fond plein d'un champ ou d'un bouton secondaire, sur une carte.
    static let control = Color(nsColor: .adaptive(light: 0xF2F2F2, dark: 0x2A2A2A))
    /// Le vert citron : interrupteurs, curseurs, étiquette « recommandé ».
    static let accent = Color(nsColor: .adaptive(light: 0xA6E35A, dark: 0xC6F36B))
    /// L'action principale : à l'encre, son texte à la couleur des cartes.
    static let primary = Color(nsColor: .adaptive(light: 0x141414, dark: 0xF2F2F2))
    static let primaryPressed = Color(nsColor: .adaptive(light: 0x3A3A3A, dark: 0xC8C8C8))
    static let onPrimary = Color(nsColor: .adaptive(light: 0xFFFFFF, dark: 0x141414))
}

/// Les tailles de texte de l'interface : 13 pt pour tout ce qu'on lit,
/// jamais moins de 12 pt.
enum Typo {
    static let pageTitle = Font.system(size: 26, weight: .heavy)
    static let sectionTitle = Font.system(size: 14, weight: .semibold)
    static let rowTitle = Font.system(size: 13, weight: .medium)
    static let body = Font.system(size: 13)
    static let detail = Font.system(size: 12)
    static let caption = Font.system(size: 12, weight: .medium)
}

// MARK: - Pages de réglages

/// Une page de réglages : un grand titre, un filet, puis ses groupes.
struct SettingsPage<Content: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text(title)
                    .font(Typo.pageTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
                if let subtitle {
                    Text(subtitle)
                        .font(Typo.body)
                        .foregroundStyle(Palette.muted)
                        .padding(.top, 4)
                        .fixedSize(horizontal: false, vertical: true)
                }
                VStack(alignment: .leading, spacing: 0) { content }
            }
            .padding(.horizontal, 36)
            .padding(.top, 40)
            .padding(.bottom, 32)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollContentBackground(.hidden)
        .background(Palette.page)
    }
}

/// Un groupe de réglages : un titre, puis ses lignes sur une carte,
/// séparées par un filet.
/// Les espacements des cartes de réglages : une seule marge intérieure,
/// partagée par les lignes, la table des modèles et les blocs.
enum Space {
    /// La marge gauche et droite du contenu d'une carte.
    static let inset: CGFloat = 20
    /// La hauteur minimale d'une ligne.
    static let row: CGFloat = 56
}

struct SettingsGroup<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content
    /// Seul dans un volet qui porte déjà son nom : pas de second titre.
    @Environment(\.settingsGroupTitleHidden) private var titleHidden

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if titleHidden {
                Color.clear.frame(height: 20).accessibilityHidden(true)
            } else {
                Text(title)
                    .font(Typo.sectionTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.leading, 2)
                    .padding(.top, 28)
                    .padding(.bottom, 10)
            }
            VStack(alignment: .leading, spacing: 0) { content }
                // Chaque ligne porte un filet au-dessus : celui de la première
                // passe sous le bord du cadre.
                .padding(.top, -1)
                .clipShape(RoundedRectangle(cornerRadius: 16))
                .background(RoundedRectangle(cornerRadius: 16).fill(Palette.sidebar))
        }
    }
}

extension EnvironmentValues {
    /// Les groupes d'un volet de réglages taisent leur titre : le volet
    /// porte déjà le nom de la section.
    @Entry var settingsGroupTitleHidden = false
}

/// Une ligne : le titre et sa description à gauche, le contrôle à droite.
struct SettingRow<Control: View>: View {
    let title: String
    var detail: String?
    @ViewBuilder let control: Control

    var body: some View {
        HStack(alignment: .center, spacing: 20) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(Typo.rowTitle)
                    .foregroundStyle(Palette.ink)
                if let detail {
                    Text(detail)
                        .font(Typo.detail)
                        .foregroundStyle(Palette.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(minWidth: 160, alignment: .leading)
            .layoutPriority(1)
            .accessibilityElement(children: .combine)
            Spacer(minLength: 12)
            control
        }
        .padding(.vertical, 14)
        .padding(.horizontal, Space.inset)
        .frame(minHeight: Space.row)
        .overlay(alignment: .top) { Rule().padding(.horizontal, Space.inset) }
    }
}

/// Le filet d'un pixel, entre deux zones.
struct Rule: View {
    var body: some View {
        Rectangle().fill(Palette.divider).frame(height: 1).accessibilityHidden(true)
    }
}

/// Un encadré : une icône, un texte, des actions dessous. Pour les avis,
/// l'offre d'une voix, un état qui demande l'attention.
struct Callout<Actions: View>: View {
    let symbol: String
    let text: String
    var identifier: String?
    var tint: Color = Palette.muted
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 14))
                .foregroundStyle(tint)
                .frame(width: 18)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 10) {
                Text(text)
                    .font(Typo.body)
                    .foregroundStyle(Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(identifier ?? "")
                actions
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 16).fill(Palette.callout))
        .padding(.top, 14)
        .accessibilityElement(children: .contain)
    }
}

extension Callout where Actions == EmptyView {
    init(symbol: String, text: String, identifier: String? = nil, tint: Color = Palette.muted) {
        self.init(symbol: symbol, text: text, identifier: identifier, tint: tint) { EmptyView() }
    }
}

/// Une étiquette de couleur (« Recommandé », « Actif »), comme les tags de Notion.
struct Tag: View {
    enum Tint { case gray, blue, green }

    let text: String
    var tint: Tint = .gray

    var body: some View {
        Text(text)
            .font(Typo.caption)
            .padding(.horizontal, 8)
            .frame(height: 20)
            .foregroundStyle(Color(nsColor: colors.text))
            .background(Capsule().fill(Color(nsColor: colors.fill)))
    }

    private var colors: (fill: NSColor, text: NSColor) {
        switch tint {
        case .gray: (.adaptive(light: 0xEAE4DA, dark: 0x3A3632), .adaptive(light: 0x2E2A25, dark: 0xE9E4DC))
        case .blue: (.adaptive(light: 0xE6E0FA, dark: 0x2E2650), .adaptive(light: 0x3A2C7A, dark: 0xDCD3FF))
        case .green: (.adaptive(light: 0xC6F36B, dark: 0xC6F36B), .adaptive(light: 0x141414, dark: 0x141414))
        }
    }
}

/// Une touche, écrite comme sur le clavier.
struct Keycap: View {
    let key: String
    var color: Color = Palette.muted
    var fill: Color = Color.primary.opacity(0.07)

    var body: some View {
        Text(key)
            .font(.system(size: 12, weight: .medium, design: .rounded))
            .foregroundStyle(color)
            .padding(.horizontal, 5)
            .frame(minWidth: 20, minHeight: 20)
            .background(RoundedRectangle(cornerRadius: 4).fill(fill))
    }
}

// MARK: - Curseur

/// La main au survol de ce qui se clique (boutons, interrupteurs, menus,
/// lignes à choisir) : on voit avant de cliquer que la zone répond. Rien
/// sur un contrôle désactivé.
struct PointingHandCursor: ViewModifier {
    var active = true
    @Environment(\.isEnabled) private var enabled

    func body(content: Content) -> some View {
        let on = active && enabled
        if #available(macOS 15, *) {
            content.pointerStyle(on ? .link : nil)
        } else {
            // `set()` à chaque mouvement : le curseur de la fenêtre ne le reprend pas.
            content.onContinuousHover { phase in
                switch phase {
                case .active where on: NSCursor.pointingHand.set()
                case .active, .ended: NSCursor.arrow.set()
                }
            }
        }
    }
}

extension View {
    /// La main au survol (voir `PointingHandCursor`).
    func pointingHandCursor(_ active: Bool = true) -> some View {
        modifier(PointingHandCursor(active: active))
    }
}

// MARK: - Boutons

/// Les boutons de l'interface : petits, pleins, à coins de 10 pt.
struct NotionButtonStyle: ButtonStyle {
    enum Kind { case primary, secondary, danger, plain }
    var kind: Kind = .secondary

    func makeBody(configuration: Configuration) -> some View {
        NotionButtonBody(configuration: configuration, kind: kind)
    }
}

extension ButtonStyle where Self == NotionButtonStyle {
    static var notion: NotionButtonStyle { NotionButtonStyle() }
    static var notionPrimary: NotionButtonStyle { NotionButtonStyle(kind: .primary) }
    static var notionPlain: NotionButtonStyle { NotionButtonStyle(kind: .plain) }
}

private struct NotionButtonBody: View {
    let configuration: ButtonStyleConfiguration
    let kind: NotionButtonStyle.Kind
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        let pressed = configuration.isPressed
        let hover = hovering && enabled
        configuration.label
            .font(.system(size: 13, weight: kind == .primary ? .semibold : .medium))
            .lineLimit(1)
            // Un bouton garde son intitulé entier : c'est le texte voisin qui passe à la ligne.
            .fixedSize(horizontal: true, vertical: false)
            .foregroundStyle(foreground)
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
            .frame(minHeight: 32)
            .background {
                let shape = RoundedRectangle(cornerRadius: 10)
                switch kind {
                case .primary:
                    shape.fill(pressed ? Palette.primaryPressed : Palette.primary)
                        .opacity(hover && !pressed ? 0.88 : 1)
                case .secondary, .danger:
                    shape.fill(pressed ? Palette.selected : (hover ? Palette.hover : Palette.control))
                case .plain:
                    shape.fill(pressed ? Palette.selected : (hover ? Palette.hover : .clear))
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 10))
            .opacity(enabled ? 1 : 0.45)
            .onHover { hovering = $0 }
            .pointingHandCursor()
    }

    private var foreground: Color {
        switch kind {
        case .primary: Palette.onPrimary
        case .danger: ErrorStyle.color
        case .secondary, .plain: Palette.ink
        }
    }
}

// MARK: - Champs

/// Un champ de saisie : plein, gris, coins de 10 pt, sans filet.
struct NotionFieldStyle: TextFieldStyle {
    func _body(configuration: TextField<Self._Label>) -> some View {
        configuration
            .textFieldStyle(.plain)
            .font(Typo.body)
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(RoundedRectangle(cornerRadius: 10).fill(Palette.control))
    }
}

extension TextFieldStyle where Self == NotionFieldStyle {
    static var notion: NotionFieldStyle { NotionFieldStyle() }
}

extension View {
    /// Un interrupteur à la taille des contrôles, sans son titre à l'écran
    /// (VoiceOver le garde).
    func notionSwitch() -> some View {
        toggleStyle(.switch).controlSize(.small).labelsHidden().tint(Palette.accent).pointingHandCursor()
    }

    /// Un curseur de largeur fixe, à la taille des contrôles.
    func notionSlider(width: CGFloat = 180) -> some View {
        controlSize(.small).labelsHidden().tint(Palette.accent).frame(width: width).pointingHandCursor()
    }
}

/// La valeur affichée à côté d'un curseur ou d'un compteur.
struct ValueText: View {
    let text: String
    var width: CGFloat = 48

    init(_ text: String, width: CGFloat = 48) {
        self.text = text
        self.width = width
    }

    var body: some View {
        Text(text)
            .font(Typo.body)
            .monospacedDigit()
            .foregroundStyle(Palette.muted)
            .frame(minWidth: width, alignment: .trailing)
            .accessibilityHidden(true)
    }
}

// MARK: - Logo

/// La marque de dysflow : une tête de profil à main levée, et dans le crâne
/// un réseau dont une branche passe le front. Même tracé que
/// `scripts/logo_gen.py`, sur la même grille de 64.
struct LogoMark: View {
    /// Le trait gras des petites tailles (sous 32 pt).
    var small = false

    var body: some View {
        Canvas { context, size in
            let scale = min(size.width, size.height) / 64
            context.translateBy(x: (size.width - 64 * scale) / 2, y: (size.height - 64 * scale) / 2)
            context.scaleBy(x: scale, y: scale)
            let shape = LogoShape(small: small)
            context.stroke(shape.furrow, with: .foreground,
                           style: StrokeStyle(lineWidth: shape.stroke, lineCap: .round, lineJoin: .round))
            // D'une seule encre, le réseau se détache du trait par un liseré
            // vide, puis se peint par-dessus.
            var halo = context
            halo.blendMode = .clear
            halo.stroke(shape.branches, with: .foreground,
                        style: StrokeStyle(lineWidth: shape.branch + 3.2, lineCap: .round))
            halo.fill(shape.nodes(grownBy: 1.6), with: .foreground)
            context.stroke(shape.branches, with: .foreground,
                           style: StrokeStyle(lineWidth: shape.branch, lineCap: .round))
            context.fill(shape.nodes(grownBy: 0), with: .foreground)
        }
        .accessibilityHidden(true)
    }

    /// La marque en image modèle, pour la barre des menus : macOS la teinte.
    @MainActor
    static func templateImage(points: CGFloat = 18) -> NSImage {
        let renderer = ImageRenderer(content: LogoMark(small: true)
            .foregroundStyle(.black)
            .frame(width: points, height: points))
        renderer.scale = 2
        let image = renderer.nsImage ?? NSImage(size: NSSize(width: points, height: points))
        image.size = NSSize(width: points, height: points)
        image.isTemplate = true
        image.accessibilityDescription = "dysflow"
        return image
    }
}

/// Le tracé, en points de la grille de 64 (celui de `scripts/logo_gen.py`).
struct LogoShape {
    let furrow: Path
    let branches: Path
    let stroke: CGFloat
    let branch: CGFloat
    /// (x, y, rayon) ; le premier est le centre du réseau.
    let points: [(CGFloat, CGFloat, CGFloat)]

    init(small: Bool) {
        stroke = small ? 4.6 : 3.2
        branch = small ? 2.4 : 1.5
        points = small
            ? [(30, 27, 5), (22.5, 20, 2.8), (52, 21.5, 3)]
            : [(30, 27, 3.8), (22.5, 21, 2), (23.5, 33.5, 1.8), (36, 18.5, 2), (33, 35, 1.7), (52, 21.5, 2.1)]
        var p = Path()
        p.move(to: CGPoint(x: 23, y: 55))
        p.addCurve(to: CGPoint(x: 16.5, y: 36), control1: CGPoint(x: 22, y: 49), control2: CGPoint(x: 17, y: 44))
        p.addCurve(to: CGPoint(x: 34, y: 12.5), control1: CGPoint(x: 15.5, y: 22), control2: CGPoint(x: 24, y: 12.5))
        p.addCurve(to: CGPoint(x: 45.5, y: 26), control1: CGPoint(x: 42, y: 12.5), control2: CGPoint(x: 46, y: 18.5))
        p.addCurve(to: CGPoint(x: 44.8, y: 30.5), control1: CGPoint(x: 45.3, y: 27.8), control2: CGPoint(x: 44.3, y: 28.8))
        p.addCurve(to: CGPoint(x: 49.8, y: 36.3), control1: CGPoint(x: 46, y: 33), control2: CGPoint(x: 49.5, y: 35))
        p.addCurve(to: CGPoint(x: 46.8, y: 38), control1: CGPoint(x: 50, y: 37.3), control2: CGPoint(x: 48, y: 37.8))
        p.addCurve(to: CGPoint(x: 46.6, y: 40.8), control1: CGPoint(x: 47.8, y: 39), control2: CGPoint(x: 47.8, y: 40))
        p.addCurve(to: CGPoint(x: 46.4, y: 43.3), control1: CGPoint(x: 47.6, y: 41.5), control2: CGPoint(x: 47.4, y: 42.6))
        p.addCurve(to: CGPoint(x: 46.6, y: 46), control1: CGPoint(x: 46, y: 44), control2: CGPoint(x: 46.6, y: 45.2))
        p.addCurve(to: CGPoint(x: 40, y: 48.4), control1: CGPoint(x: 46.4, y: 48), control2: CGPoint(x: 43, y: 48.6))
        p.addCurve(to: CGPoint(x: 39.8, y: 55.5), control1: CGPoint(x: 38.6, y: 50.5), control2: CGPoint(x: 39, y: 53))
        furrow = p
        var b = Path()
        let (cx, cy, _) = points[0]
        for (x, y, _) in points.dropFirst() {
            b.move(to: CGPoint(x: cx, y: cy))
            b.addLine(to: CGPoint(x: x, y: y))
        }
        branches = b
    }

    func nodes(grownBy margin: CGFloat) -> Path {
        var p = Path()
        for (x, y, r) in points {
            let radius = r + margin
            p.addEllipse(in: CGRect(x: x - radius, y: y - radius, width: radius * 2, height: radius * 2))
        }
        return p
    }
}

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
            VStack(alignment: .leading, spacing: 3) {
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
        .padding(.vertical, 10)
        .padding(.horizontal, 16)
        .frame(minHeight: 48)
        .overlay(alignment: .top) { Rule().padding(.horizontal, 16) }
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
            .padding(.horizontal, 12)
            .frame(minHeight: 30)
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
        toggleStyle(.switch).controlSize(.small).labelsHidden().tint(Palette.accent)
    }

    /// Un curseur de largeur fixe, à la taille des contrôles.
    func notionSlider(width: CGFloat = 180) -> some View {
        controlSize(.small).labelsHidden().tint(Palette.accent).frame(width: width)
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

/// La marque de sillon : un « s » d'un seul trait, comme un sillon de
/// labour, et la tête de lecture au bout. Même tracé que
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
            context.fill(shape.dot, with: .foreground)
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
        image.accessibilityDescription = "sillon"
        return image
    }
}

/// Le tracé, en points de la grille de 64.
struct LogoShape {
    let furrow: Path
    let dot: Path
    let stroke: CGFloat

    init(small: Bool) {
        let (width, gap, stroke, point, play): (CGFloat, CGFloat, CGFloat, CGFloat, CGFloat) =
            small ? (24, 15, 7, 4.6, 3.4) : (26, 13, 5, 3.4, 3.6)
        let r = gap / 2
        let left = 32 - width / 2, right = 32 + width / 2
        let top = 32 - gap, middle: CGFloat = 32, bottom = 32 + gap
        var p = Path()
        p.move(to: CGPoint(x: right, y: top))
        p.addLine(to: CGPoint(x: left, y: top))
        p.addArc(center: CGPoint(x: left, y: top + r), radius: r,
                 startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: true)
        p.addLine(to: CGPoint(x: right, y: middle))
        p.addArc(center: CGPoint(x: right, y: middle + r), radius: r,
                 startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: false)
        p.addLine(to: CGPoint(x: left + point + play + stroke / 2, y: bottom))
        furrow = p
        dot = Path(ellipseIn: CGRect(x: left - point, y: bottom - point, width: point * 2, height: point * 2))
        self.stroke = stroke
    }
}

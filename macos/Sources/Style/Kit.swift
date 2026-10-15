import AppKit
import SwiftUI

// Le style de l'app, à la manière de Notion (14 oct. 2026) : une encre
// chaude sur blanc, des gris pour tout ce qui est secondaire, des filets fins,
// des fonds de survol à peine marqués, le bleu réservé à l'action et au mot lu.
// Les contrôles restent petits : ils ne grandissent jamais avec le texte lu.

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
    static let page = Color(nsColor: .adaptive(light: 0xFFFFFF, dark: 0x191919))
    static let sidebar = Color(nsColor: .adaptive(light: 0xF7F7F5, dark: 0x202020))
    /// Le texte courant : 11:1 sur le blanc, 10:1 sur le fond sombre.
    static let ink = Color(nsColor: .adaptive(light: 0x37352F, dark: 0xD4D4D4))
    /// Descriptions et légendes : 5:1 au moins sur les deux fonds (WCAG AA).
    static let muted = Color(nsColor: .adaptive(light: 0x6B6A66, dark: 0x9B9B9B))
    /// Les icônes seulement, jamais du texte.
    static let faint = Color(nsColor: .adaptive(light: 0x91908C, dark: 0x7D7D7D))
    static let divider = Color(nsColor: .adaptive(light: 0xE9E9E7, dark: 0x2F2F2F))
    static let hover = Color(nsColor: .adaptive(light: 0xEFEFED, dark: 0x2A2A2A))
    static let selected = Color(nsColor: .adaptive(light: 0xE8E8E6, dark: 0x2F2F2F))
    /// Le fond d'un encadré (« callout ») ou d'un champ.
    static let callout = Color(nsColor: .adaptive(light: 0xF4F4F2, dark: 0x252525))
    static let accent = Color(nsColor: .adaptive(light: 0x2383E2, dark: 0x2383E2))
    static let accentPressed = Color(nsColor: .adaptive(light: 0x0F6DCA, dark: 0x0F6DCA))
}

/// Les tailles de texte de l'interface : 13 pt pour tout ce qu'on lit,
/// jamais moins de 12 pt.
enum Typo {
    static let pageTitle = Font.system(size: 22, weight: .bold)
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
                Rule().padding(.top, 14)
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

/// Un groupe de réglages : un titre souligné, des lignes dessous.
struct SettingsGroup<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(Typo.sectionTitle)
                .foregroundStyle(Palette.ink)
                .accessibilityAddTraits(.isHeader)
                .padding(.top, 30)
                .padding(.bottom, 8)
            Rule()
            VStack(alignment: .leading, spacing: 0) { content }
                .padding(.top, 4)
        }
    }
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
        .frame(minHeight: 44)
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
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 6).fill(Palette.callout))
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
            .padding(.horizontal, 6)
            .frame(height: 20)
            .foregroundStyle(Color(nsColor: colors.text))
            .background(RoundedRectangle(cornerRadius: 4).fill(Color(nsColor: colors.fill)))
    }

    private var colors: (fill: NSColor, text: NSColor) {
        switch tint {
        case .gray: (.adaptive(light: 0xE3E2E0, dark: 0x373737), .adaptive(light: 0x32302C, dark: 0xD4D4D4))
        case .blue: (.adaptive(light: 0xD3E5EF, dark: 0x1F3A52), .adaptive(light: 0x183347, dark: 0xD3E5EF))
        case .green: (.adaptive(light: 0xDBEDDB, dark: 0x1F3A2A), .adaptive(light: 0x1C3829, dark: 0xD0EBD6))
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

/// Les boutons de l'interface : petits, à coins de 6 pt, fond au survol.
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
            .padding(.horizontal, 10)
            .frame(minHeight: 28)
            .background {
                let shape = RoundedRectangle(cornerRadius: 6)
                switch kind {
                case .primary:
                    shape.fill(pressed ? Palette.accentPressed : Palette.accent)
                        .brightness(hover && !pressed ? -0.04 : 0)
                case .secondary, .danger:
                    shape.fill(pressed ? Palette.selected : (hover ? Palette.hover : Palette.page))
                        .overlay(shape.strokeBorder(Palette.divider.opacity(1), lineWidth: 1))
                        .overlay(shape.strokeBorder(Color.primary.opacity(0.06), lineWidth: 1))
                case .plain:
                    shape.fill(pressed ? Palette.selected : (hover ? Palette.hover : .clear))
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: 6))
            .opacity(enabled ? 1 : 0.45)
            .onHover { hovering = $0 }
    }

    private var foreground: Color {
        switch kind {
        case .primary: .white
        case .danger: ErrorStyle.color
        case .secondary, .plain: Palette.ink
        }
    }
}

// MARK: - Champs

/// Un champ de saisie : fond gris léger, filet fin, coins de 6 pt.
struct NotionFieldStyle: TextFieldStyle {
    func _body(configuration: TextField<Self._Label>) -> some View {
        configuration
            .textFieldStyle(.plain)
            .font(Typo.body)
            .padding(.horizontal, 8)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: 6).fill(Palette.callout))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.divider, lineWidth: 1))
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

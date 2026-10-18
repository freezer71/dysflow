import SwiftUI

/// La fenêtre de dictée : une petite carte sobre en bas de l'écran. En
/// haut, l'icône de l'application où le texte ira, au centre huit barres
/// fines qui suivent la voix, à droite le délai du dernier texte ; dessous,
/// les derniers mots entendus sur deux lignes (les provisoires plus pâles),
/// qui s'écrivent lettre à lettre. Le fond de la carte, son
/// filet et son ouverture sont dessinés par `DictationWindow` (Core Animation). Aucun bouton : le raccourci
/// saisit, Échap annule ; seul « Autoriser » paraît quand il le faut.
struct DictationView: View {
    let controller: DictationController
    @Environment(\.colorScheme) private var colorScheme
    @State private var increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast

    private var theme: Theme {
        Theme.resolve(dark: colorScheme == .dark, increaseContrast: increaseContrast)
    }

    var body: some View {
        VStack(spacing: 6) {
            // Une ligne : l'application à gauche, les barres au centre.
            ZStack {
                Group {
                    // Une autorisation manque : le bouton prend la place des barres.
                    if let permission = controller.missingPermission {
                        Button("Autoriser") { controller.requestMissingPermission() }
                            .buttonStyle(.notionPrimary)
                            .controlSize(.mini)
                            .accessibilityLabel("Autoriser : \(permission.title)")
                            .accessibilityIdentifier("autoriser-dictee")
                    } else {
                        VoiceBars(feed: controller.spectrum, phase: controller.phase, theme: theme)
                    }
                }
                TargetIcon(target: controller.target)
                    .frame(maxWidth: .infinity, alignment: .leading)
                LatencyLabel(latency: controller.latency, theme: theme)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .frame(height: VoiceBars.tallest)
            DictationLine(controller: controller, theme: theme)
        }
        .padding(.horizontal, DictationMetrics.padding)
        .padding(.vertical, 10)
        .frame(width: DictationMetrics.width, height: DictationMetrics.height, alignment: .top)
        .frame(width: DictationMetrics.windowWidth, height: DictationMetrics.windowHeight)
        .onReceive(NSWorkspace.shared.notificationCenter.publisher(
            for: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification)) { _ in
            increaseContrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Dictée")
    }
}

/// L'icône de l'application où le texte sera saisi.
struct TargetIcon: View {
    let target: DictationTarget?

    var body: some View {
        Group {
            if let icon = target?.icon {
                Image(nsImage: icon).resizable().interpolation(.high)
            } else {
                Image(systemName: "text.cursor").font(.system(size: 10, weight: .medium))
            }
        }
        .frame(width: 14, height: 14)
        .help(target.map { "Saisie dans \($0.name)" } ?? "")
        .accessibilityLabel(target.map { "Saisie dans \($0.name)" } ?? "Saisie")
    }
}

/// Le délai du dernier texte, discret, à droite des barres : rien tant
/// qu'il n'est pas connu.
struct LatencyLabel: View {
    let latency: Duration?
    let theme: Theme

    var body: some View {
        if let latency {
            Text(Self.format(latency))
                .font(.system(size: 10, weight: .medium).monospacedDigit())
                .foregroundStyle(Color(nsColor: theme.chrome).opacity(0.8))
                .help("Délai : de la fin de ce qui a été dit au texte affiché")
                .accessibilityLabel("Délai : \(Self.format(latency))")
                .accessibilityIdentifier("delai-dictee")
        }
    }

    /// « 120 ms » sous la seconde, « 1,4 s » au-delà.
    static func format(_ latency: Duration) -> String {
        let ms = Double(latency.components.seconds) * 1000 + Double(latency.components.attoseconds) / 1e15
        if ms < 1000 { return "\(Int(ms.rounded())) ms" }
        return (ms / 1000).formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: "fr_FR"))) + " s"
    }
}

// MARK: - Barres

/// Huit barres arrondies, centrées : chacune suit une bande de fréquence de la voix,
/// les plus fortes au centre. Elles montent vite et retombent doucement ; au
/// repos, ce sont huit petits traits.
struct VoiceBars: View {
    let feed: SpectrumFeed
    let phase: DictationController.Phase
    let theme: Theme
    @State private var motion = BarMotion()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let count = BarMotion.count
    static let width: CGFloat = 2.5
    static let gap: CGFloat = 3
    static let rest: CGFloat = 4
    static let tallest: CGFloat = 16

    var body: some View {
        let live = phase == .listening
        TimelineView(.animation(minimumInterval: 1.0 / 60, paused: !live)) { context in
            Canvas { canvas, size in
                let now = context.date.timeIntervalSinceReferenceDate
                if live { feed.advance(to: now) }
                let target = live ? Self.levels(feed.columns.last ?? []) : []
                let levels = motion.step(toward: target, at: now, smooth: !reduceMotion)
                var path = Path()
                for i in 0..<Self.count {
                    let h = Self.rest + (Self.tallest - Self.rest) * CGFloat(levels[i])
                    let x = CGFloat(i) * (Self.width + Self.gap)
                    path.addRoundedRect(in: CGRect(x: x, y: (size.height - h) / 2, width: Self.width, height: h),
                                        cornerSize: CGSize(width: Self.width / 2, height: Self.width / 2))
                }
                canvas.fill(path, with: .color(Color(nsColor: theme.text).opacity(live ? 0.9 : 0.4)))
            }
        }
        .frame(width: CGFloat(Self.count) * Self.width + CGFloat(Self.count - 1) * Self.gap,
               height: Self.tallest)
        .accessibilityHidden(true)
    }

    /// Les huit hauteurs (0 à 1) d'une tranche du spectre : les bandes
    /// regroupées, les plus graves (les plus fortes) au centre.
    static func levels(_ column: [Float]) -> [Float] {
        guard !column.isEmpty else { return Array(repeating: 0, count: count) }
        let grouped = (0..<count).map { i -> Float in
            let lo = i * column.count / count
            let hi = min(column.count, max(lo + 1, (i + 1) * column.count / count))
            return column[lo..<hi].max() ?? 0
        }
        // Une colline : 6 4 2 0 1 3 5 7.
        return [6, 4, 2, 0, 1, 3, 5, 7].map { grouped[$0] }
    }
}

/// Le mouvement des barres : une montée rapide, une retombée douce.
final class BarMotion {
    nonisolated static let count = 8
    private(set) var levels = [Float](repeating: 0, count: BarMotion.count)
    private var last: TimeInterval?

    func step(toward target: [Float], at now: TimeInterval, smooth: Bool) -> [Float] {
        defer { last = now }
        let goal = target.count == levels.count ? target : [Float](repeating: 0, count: levels.count)
        guard smooth, let last else {
            levels = goal
            return levels
        }
        let dt = Float(min(0.1, max(0, now - last)))
        for i in levels.indices {
            let rate: Float = goal[i] > levels[i] ? 28 : 9
            levels[i] += (goal[i] - levels[i]) * min(1, rate * dt)
        }
        return levels
    }
}

// MARK: - Texte

/// Les derniers mots entendus, sur deux lignes centrées (le début s'efface
/// par l'avant) ; ou un message. Les lettres nouvelles s'écrivent une à une
/// (`TypingReveal`) au lieu d'arriver d'un bloc à chaque relecture.
struct DictationLine: View {
    let controller: DictationController
    let theme: Theme
    @State private var reveal = TypingReveal()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let placeholder = "Parlez, le texte s'écrit ici."

    /// Ce que disent les deux lignes à la place du texte.
    static func status(_ phase: DictationController.Phase) -> String? {
        switch phase {
        case .preparing(let download?): "Installation de la langue… \(Int((download * 100).rounded())) %"
        case .notice(let message), .failed(let message): message
        case .preparing, .listening, .finishing, .hidden: nil
        }
    }

    var body: some View {
        Group {
            if let status = Self.status(controller.phase) {
                Text(status)
                    .foregroundStyle(Color(nsColor: theme.text))
                    .accessibilityIdentifier("etat-dictee")
            } else if controller.transcript.isEmpty {
                Text(Self.placeholder)
                    .foregroundStyle(Color(nsColor: theme.chrome).opacity(0.7))
            } else {
                let transcript = controller.transcript
                // Au pas de l'écran tant que des lettres restent à écrire.
                TimelineView(.animation(minimumInterval: 1.0 / 60,
                                        paused: !reveal.isTyping(transcript.text))) { context in
                    let shown = reveal.visible(transcript.text, at: context.date.timeIntervalSinceReferenceDate,
                                               animated: !reduceMotion)
                    Text(Self.styled(transcript, theme: theme, from: reveal.cut(shown), upTo: shown.count))
                }
            }
        }
        .font(.system(size: Self.fontSize, weight: .medium))
        .lineSpacing(Self.lineSpacing)
        .lineLimit(2)
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .frame(height: DictationMetrics.textHeight)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("texte-dicte")
    }

    static let fontSize: CGFloat = 12
    static let lineSpacing: CGFloat = 1
    /// La largeur du texte : la carte moins ses marges, et une réserve.
    static let width: CGFloat = DictationMetrics.width - 2 * DictationMetrics.padding - 8

    /// Où commence ce qui tient sur deux lignes : 0 si tout tient ; sinon le
    /// début du premier mot gardé (la fin du texte reste visible).
    static func cut(_ text: String, width: CGFloat, lines: Int = 2) -> Int {
        let font = NSFont.systemFont(ofSize: fontSize, weight: .medium)
        let style = NSMutableParagraphStyle()
        style.lineSpacing = lineSpacing
        func height(_ s: String) -> CGFloat {
            NSAttributedString(string: s, attributes: [.font: font, .paragraphStyle: style])
                .boundingRect(with: CGSize(width: width, height: .greatestFiniteMagnitude),
                              options: [.usesLineFragmentOrigin, .usesFontLeading]).height
        }
        let line = height("A")
        let limit = CGFloat(lines) * line + CGFloat(lines - 1) * lineSpacing + 1
        if height(text) <= limit { return 0 }
        // Les débuts de mots, du dernier au premier : on garde le plus de mots possible.
        let characters = Array(text)
        let starts = characters.indices.filter { $0 > 0 && characters[$0 - 1] == " " && characters[$0] != " " }
        var best = starts.last ?? 0
        var lo = 0, hi = starts.count - 1
        while lo <= hi {
            let mid = (lo + hi) / 2
            if height("…" + String(characters[starts[mid]...])) <= limit {
                best = starts[mid]
                hi = mid - 1
            } else {
                lo = mid + 1
            }
        }
        return best
    }

    /// Le texte arrêté à l'encre ; les mots provisoires plus pâles. `from` :
    /// le début de ce qui s'affiche (en caractères), précédé de « … » ;
    /// `upTo` : sa fin (les lettres pas encore écrites), tout par défaut.
    static func styled(_ transcript: Transcript, theme: Theme, from start: Int = 0,
                       upTo end: Int? = nil) -> AttributedString {
        typealias Ink = AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute
        let settledCount = transcript.settled.count
        let full = String(transcript.text.prefix(end ?? Int.max))
        let start = min(max(0, start), full.count)
        let index = full.index(full.startIndex, offsetBy: start)
        // La part arrêtée finit au même endroit dans `text` (puis une espace).
        let settledEnd = full.index(full.startIndex, offsetBy: min(settledCount, full.count))
        var result = AttributedString(start > 0 ? "…" : "")
        result[Ink.self] = Color(nsColor: theme.text)
        if index < settledEnd {
            var settled = AttributedString(String(full[index..<settledEnd]))
            settled[Ink.self] = Color(nsColor: theme.text)
            result += settled
        }
        var rest = AttributedString(String(full[max(index, settledEnd)...]))
        rest[Ink.self] = Color(nsColor: theme.chrome)
        result += rest
        return result
    }
}


/// L'écriture lettre à lettre des mots nouveaux. Ce qui ne change pas reste ;
/// une relecture qui corrige un mot déjà écrit repart de la première lettre
/// qui diffère ; le reste s'écrit en `catchUp` au plus (25 lettres par
/// seconde au moins), pour finir avant la relecture suivante. Sans
/// animation (Réduire les animations) : tout, tout de suite.
///
/// `idle` est observé : il passe à vrai (hors du rendu en cours) quand tout
/// est écrit, et l'horloge de `DictationLine` se met en pause.
@MainActor @Observable
final class TypingReveal {
    static let catchUp: Double = 0.5
    static let minimumRate: Double = 25

    private(set) var idle = true
    @ObservationIgnored private var target = ""
    @ObservationIgnored private var shown: Double = 0
    @ObservationIgnored private var rate: Double = minimumRate
    @ObservationIgnored private var last: TimeInterval?
    @ObservationIgnored private var cutCache: (text: String, start: Int)?

    /// Vrai s'il reste des lettres de `text` à écrire.
    func isTyping(_ text: String) -> Bool {
        text != target || !idle
    }

    /// Ce qui est écrit de `text` à l'instant `now`.
    func visible(_ text: String, at now: TimeInterval, animated: Bool) -> String {
        if text != target {
            let common = zip(target, text).prefix { $0 == $1 }.count
            target = text
            shown = min(shown, Double(common))
            rate = max(Self.minimumRate, (Double(text.count) - shown) / Self.catchUp)
        }
        if !animated {
            shown = Double(target.count)
        } else if let last {
            shown = min(Double(target.count), shown + rate * min(0.1, max(0, now - last)))
        }
        last = now
        let done = shown >= Double(target.count)
        // Pas pendant le rendu : au tour suivant.
        if done != idle { Task { @MainActor in self.idle = done } }
        return String(target.prefix(Int(shown)))
    }

    /// `DictationLine.cut` de ce texte, gardé tant qu'il ne change pas (la
    /// mesure ne se refait pas à chaque image).
    func cut(_ text: String) -> Int {
        if let cutCache, cutCache.text == text { return cutCache.start }
        let start = DictationLine.cut(text, width: DictationLine.width)
        cutCache = (text, start)
        return start
    }
}

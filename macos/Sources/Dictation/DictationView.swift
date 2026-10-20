import SwiftUI

/// La fenêtre de dictée : une petite carte sobre en bas de l'écran. En
/// haut, l'icône de l'application où le texte ira, au centre huit barres
/// fines qui suivent la voix, à droite le délai du dernier texte et la
/// mémoire occupée ; dessous,
/// les derniers mots entendus sur deux lignes (les provisoires plus pâles),
/// qui s'écrivent lettre à lettre. Le fond de la carte, son
/// filet et son ouverture sont dessinés par `DictationWindow` (Core Animation). Aucun bouton : le raccourci
/// saisit, Échap annule ; seul « Autoriser » paraît quand il le faut.
struct DictationView: View {
    let controller: DictationController
    @AppStorage(DictationStats.defaultsKey) private var showStats = true
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
                        Button("Allow") { controller.requestMissingPermission() }
                            .buttonStyle(.notionPrimary)
                            .controlSize(.mini)
                            .accessibilityLabel("Allow: \(permission.title)")
                            .accessibilityIdentifier("autoriser-dictee")
                    } else {
                        VoiceBars(feed: controller.spectrum, phase: controller.phase, theme: theme)
                    }
                }
                TargetIcon(target: controller.target)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if showStats {
                    DictationStats(latency: controller.latency, memory: controller.memory, theme: theme)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
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
        .accessibilityLabel("Dictation")
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
        .help(target.map { String(localized: "Typing into \($0.name)") } ?? "")
        .accessibilityLabel(target.map { String(localized: "Typing into \($0.name)") } ?? String(localized: "Typing"))
    }
}

/// Le délai du dernier texte et la mémoire que MLX occupe, discrets, à
/// droite des barres (« 120 ms · 506 Mo ») : chacun absent tant qu'il n'est
/// pas connu. Le réglage « Délai et mémoire » (section Dictée) les masque.
struct DictationStats: View {
    /// Montrer le délai et la mémoire : oui, sauf réglage contraire.
    static let defaultsKey = "dictationShowStats"

    static func shown(_ defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: defaultsKey) as? Bool ?? true
    }

    let latency: Duration?
    let memory: Int?
    let theme: Theme

    var body: some View {
        if let text = Self.text(latency: latency, memory: memory) {
            Text(text)
                .font(.system(size: 10, weight: .medium).monospacedDigit())
                .foregroundStyle(Color(nsColor: theme.chrome).opacity(0.8))
                .help(Self.help(latency: latency, memory: memory))
                .accessibilityLabel(Self.help(latency: latency, memory: memory))
                .accessibilityIdentifier("mesures-dictee")
        }
    }

    static func text(latency: Duration?, memory: Int?) -> String? {
        let parts = [latency.map(format), memory.map(size)].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    static func help(latency: Duration?, memory: Int?) -> String {
        [latency.map { String(localized: "Delay: \(format($0)), from the end of what was said to the text shown") },
         memory.map { String(localized: "Memory: \(size($0)), used by the model and its computations") }]
            .compactMap { $0 }.joined(separator: ". ")
    }

    /// « 120 ms » sous la seconde, « 1,4 s » au-delà.
    static func format(_ latency: Duration) -> String {
        let ms = Double(latency.components.seconds) * 1000 + Double(latency.components.attoseconds) / 1e15
        if ms < 1000 { return "\(Int(ms.rounded())) ms" }
        return (ms / 1000).formatted(.number.precision(.fractionLength(1)).locale(UILanguage.locale)) + " s"
    }

    /// Comme le reste des réglages (« 506 Mo »).
    static func size(_ bytes: Int) -> String {
        SizeFormat.bytes(UInt64(max(0, bytes)))
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

/// Les derniers mots entendus, sur deux lignes alignées à gauche (quand
/// elles sont pleines, le texte monte d'une ligne) ; ou un message, centré.
/// Les lettres nouvelles s'écrivent une à une (`TypingReveal`) au lieu
/// d'arriver d'un bloc à chaque relecture.
struct DictationLine: View {
    let controller: DictationController
    let theme: Theme
    @State private var reveal = TypingReveal()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let placeholder = String(localized: "Speak, and the text appears here.")

    /// Ce que disent les deux lignes à la place du texte.
    static func status(_ phase: DictationController.Phase) -> String? {
        switch phase {
        case .preparing(let download?): String(localized: "Installing the language… \(Int((download * 100).rounded()))%")
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
                    Text(Self.styled(transcript, theme: theme, from: reveal.start(of: shown), upTo: shown.count))
                        // À gauche : une lettre ajoutée ne déplace pas celles déjà écrites.
                        .multilineTextAlignment(.leading)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
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
        // Une nouvelle dictée part d'un texte vide : l'écriture repart de zéro.
        .onChange(of: controller.transcript.isEmpty) { _, empty in
            if empty { reveal.reset() }
        }
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

    /// Où commence la deuxième ligne de `text` affiché sur `width` (en
    /// caractères) ; `nil` s'il tient sur une ligne.
    static func secondLineStart(_ text: String, width: CGFloat) -> Int? {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = lineSpacing
        let storage = NSTextStorage(string: text, attributes: [
            .font: NSFont.systemFont(ofSize: fontSize, weight: .medium), .paragraphStyle: style])
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: CGSize(width: width, height: .greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        var range = NSRange()
        layout.lineFragmentRect(forGlyphAt: 0, effectiveRange: &range)
        let next = NSMaxRange(range)
        guard next < layout.numberOfGlyphs else { return nil }
        // En caractères Swift (une lettre accentuée peut compter deux unités UTF-16).
        let utf16 = layout.characterIndexForGlyph(at: next)
        return String(text.utf16.prefix(utf16))?.count
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


/// L'écriture lettre à lettre des mots nouveaux : le texte ne fait que
/// s'allonger.
///
/// - Les mots déjà écrits restent écrits : une relecture qui en corrige un
///   (« Yeah » → « Yes ») le remplace sur place, sans rien réécrire ; seuls
///   les mots en plus s'écrivent, en `catchUp` au plus (25 lettres par
///   seconde au moins), pour finir avant la relecture suivante.
/// - Sur deux lignes pleines, le texte monte d'une ligne entière (comme un
///   prompteur), au lieu de se redistribuer à chaque mot.
/// - Sans animation (Réduire les animations) : tout, tout de suite.
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
    /// Le premier mot affiché (les précédents sont montés hors de la carte).
    @ObservationIgnored private var firstWord = 0
    @ObservationIgnored private var startCache: (text: String, start: Int)?

    /// Une nouvelle dictée : rien d'écrit.
    func reset() {
        target = ""
        shown = 0
        last = nil
        firstWord = 0
        startCache = nil
    }

    /// Vrai s'il reste des lettres de `text` à écrire.
    func isTyping(_ text: String) -> Bool {
        text != target || !idle
    }

    /// Ce qui est écrit de `text` à l'instant `now`.
    func visible(_ text: String, at now: TimeInterval, animated: Bool) -> String {
        if text != target {
            shown = Double(Self.kept(old: target, shown: Int(shown), new: text))
            target = text
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

    /// Ce qui reste écrit de `new` quand il remplace `old`, dont `shown`
    /// lettres étaient écrites : le début commun, et au moins autant de mots
    /// entiers qu'il y en avait d'écrits (corrigés sur place).
    static func kept(old: String, shown: Int, new: String) -> Int {
        let common = min(shown, zip(old, new).prefix { $0 == $1 }.count)
        let written = old.prefix(shown)
        // Les mots écrits en entier : tous si l'écriture était au bout ou à
        // une espace, sinon tous sauf le dernier, entamé.
        var words = written.split(separator: " ").count
        if shown < old.count, !written.isEmpty, !written.hasSuffix(" ") { words -= 1 }
        return min(new.count, max(common, endOfWords(words, in: new)))
    }

    /// La fin (en caractères) des `count` premiers mots de `text`.
    static func endOfWords(_ count: Int, in text: String) -> Int {
        guard count > 0 else { return 0 }
        var seen = 0, inWord = false
        for (i, c) in text.enumerated() {
            if c == " " {
                if inWord { seen += 1; if seen == count { return i } }
                inWord = false
            } else {
                inWord = true
            }
        }
        return text.count
    }

    /// Où commence l'affichage de `text` (en caractères) : au premier mot
    /// gardé. Sur deux lignes pleines, il avance d'une ligne entière ; il ne
    /// recule jamais pendant une dictée. Gardé tant que le texte ne change pas
    /// (la mesure ne se refait pas à chaque image).
    func start(of text: String, width: CGFloat = DictationLine.width) -> Int {
        if let startCache, startCache.text == text { return startCache.start }
        var start = Self.endOfWords(firstWord, in: text)
        if start > 0, start < text.count { start += 1 } // après l'espace
        if start >= text.count { start = 0; firstWord = 0 }
        while DictationLine.cut(String(text.dropFirst(start)), width: width - Self.ellipsisWidth(start)) > 0 {
            // Deux lignes pleines : la première monte hors de la carte.
            let shown = (start > 0 ? "…" : "") + String(text.dropFirst(start))
            guard let next = DictationLine.secondLineStart(shown, width: width) else { break }
            let advance = next - (start > 0 ? 1 : 0)
            guard advance > 0 else { break }
            firstWord += String(text.dropFirst(start).prefix(advance)).split(separator: " ").count
            start = Self.endOfWords(firstWord, in: text)
            if start < text.count { start += 1 }
        }
        startCache = (text, start)
        return start
    }

    /// La place de « … » devant un texte coupé.
    private static func ellipsisWidth(_ start: Int) -> CGFloat {
        guard start > 0 else { return 0 }
        return NSAttributedString(string: "…", attributes: [
            .font: NSFont.systemFont(ofSize: DictationLine.fontSize, weight: .medium)]).size().width
    }
}

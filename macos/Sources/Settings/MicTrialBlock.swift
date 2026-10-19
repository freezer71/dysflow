import AppKit
import SwiftUI

/// L'essai du micro, sous la table des modèles de dictée : une phrase à
/// lire, le niveau du micro, le texte reconnu en direct (jamais saisi), puis
/// les résultats du modèle essayé, et les derniers de chaque modèle pour
/// comparer. Le modèle essayé se choisit ici sans changer celui de la dictée.
struct MicTrialBlock: View {
    let trial: MicTrialController
    /// Les modèles (lecture et dictée), tels que le moteur les donne.
    let rows: [VoiceModel]
    /// Le modèle de la dictée (`dictationModel`) : le choix par défaut.
    let chosen: String
    /// Une dictée est en cours : l'essai attend.
    let dictating: Bool
    /// `nil` : celui de la dictée.
    @State private var picked: String?
    /// La langue de la dictée, changée juste en dessous : la phrase la suit.
    @AppStorage(DictationLanguage.defaultsKey) private var language = DictationLanguage.stored().rawValue

    /// La dictée de macOS, puis les modèles de dictée installés.
    nonisolated static func options(_ rows: [VoiceModel]) -> [(title: String, value: String)] {
        [(DictationModel.label(DictationModel.system), DictationModel.system)]
            + rows.filter { $0.kind == "dictation" && $0.installed }.map { ($0.name, $0.id) }
    }

    /// Le modèle essayé : celui choisi ici s'il est encore là, sinon celui
    /// de la dictée, sinon la dictée de macOS.
    nonisolated static func selected(_ picked: String?, chosen: String,
                                     options: [(title: String, value: String)]) -> String {
        let ids = options.map(\.value)
        if let picked, ids.contains(picked) { return picked }
        return ids.contains(chosen) ? chosen : DictationModel.system
    }

    private var options: [(title: String, value: String)] { Self.options(rows) }
    private var selection: String { Self.selected(picked, chosen: chosen, options: options) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Microphone trial")
                    .font(Typo.sectionTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
                Text("Read the sentence aloud: the trial stops by itself once it has been said, or when you go quiet. The recognized text appears here: it is not typed anywhere. The model you try does not change the one used for dictation.")
                    .font(Typo.body)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 20) {
                HStack(spacing: 8) {
                    Text("Model")
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                        .accessibilityHidden(true)
                    PopUpPicker(label: String(localized: "Trial model"), identifier: "essai-modele", options: options,
                                selection: Binding(get: { selection }, set: { picked = $0 }))
                        .fixedSize()
                        .disabled(trial.isRunning)
                }
                Toggle(isOn: Binding(get: { trial.freeText }, set: { trial.freeText = $0 })) {
                    Text("Free text")
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                }
                .notionSwitch()
                .disabled(trial.isRunning)
                .accessibilityLabel("Free text: say whatever you like, with no accuracy score")
                .accessibilityIdentifier("essai-texte-libre")
                Spacer(minLength: 0)
            }
            sentenceBox
            controls
            heardBox
            if let id = trial.modelID, let result = trial.results[id], !trial.isRunning {
                MicTrialResultCard(name: DictationModel.label(id), result: result)
            }
            MicTrialHistory(results: trial.results, options: options)
        }
        .padding(.horizontal, Space.inset)
        .padding(.vertical, 20)
        .overlay(alignment: .top) { Rule().padding(.horizontal, Space.inset) }
        .onAppear { trial.refreshLanguage() }
        .onChange(of: language) { _, _ in trial.refreshLanguage() }
        // La section change ou la fenêtre se ferme : le micro se ferme.
        .onDisappear { trial.cancel() }
        // Une fenêtre de sillage réduite dans le Dock (les réglages, le plus
        // souvent) : le micro se ferme.
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.willMiniaturizeNotification)) { _ in
            if trial.isRunning { trial.cancel() }
        }
        // VoiceOver suit l'essai : le micro qui écoute, le texte final qui
        // arrive, un échec ; puis les résultats (`announcement`).
        .onChange(of: trial.phase) { _, phase in
            if let text = Self.spoken(phase) { Announcement.post(text) }
        }
        .onChange(of: trial.announcement) { _, text in
            if let text { Announcement.post(text) }
        }
    }

    @ViewBuilder private var sentenceBox: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(trial.freeText ? "Say whatever you like" : "Sentence to read")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
            if trial.freeText {
                Text("Speak freely: accuracy is not computed; delays, throughput and memory are.")
                    .font(.system(size: 17))
                    .foregroundStyle(Palette.ink)
                    .lineSpacing(5)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                Text(trial.sentence)
                    .font(.system(size: 24, weight: .medium))
                    .foregroundStyle(Palette.ink)
                    .lineSpacing(8)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                    .accessibilityLabel("Sentence to read: \(trial.sentence)")
                    .accessibilityIdentifier("essai-phrase")
                Button("Another sentence") { trial.nextSentence() }
                    .buttonStyle(.notionPlain)
                    .disabled(trial.isRunning)
                    .accessibilityIdentifier("essai-autre-phrase")
            }
        }
        .padding(Space.inset)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12).fill(Palette.control))
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 16) {
                MicLevelMeter(feed: trial.spectrum, live: trial.phase == .listening)
                    .accessibilityElement()
                    .accessibilityLabel("Microphone level")
                    .accessibilityValue(meterValue)
                    .accessibilityIdentifier("essai-niveau")
                Text(status)
                    .font(Typo.body)
                    .foregroundStyle(statusIsError ? ErrorStyle.color : Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("essai-etat")
                Spacer(minLength: 12)
                Button(trial.isRunning ? "Stop" : "Start the trial") {
                    if trial.isRunning { trial.stop() } else { trial.start(model: selection) }
                }
                .buttonStyle(.notionPrimary)
                .fixedSize()
                .disabled(!trial.isRunning && dictating || trial.phase == .finishing)
                .accessibilityLabel(trial.isRunning ? "Stop the trial" : "Start the microphone trial")
                .accessibilityIdentifier("essai-demarrer")
            }
            if dictating && !trial.isRunning {
                Text(MicTrialController.dictationBusy)
                    .font(Typo.body)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var heardBox: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Recognized text")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
            Text(trial.transcript.isEmpty ? String(localized: "The text will appear here while you speak.") : trial.transcript.text)
                .font(.system(size: 17))
                .foregroundStyle(trial.transcript.isEmpty ? Palette.muted : Palette.ink)
                .lineSpacing(5)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, minHeight: 48, alignment: .topLeading)
                .textSelection(.enabled)
                .accessibilityLabel("Recognized text: \(heardSpoken)")
                .accessibilityIdentifier("essai-texte")
        }
    }

    private var heardSpoken: String {
        trial.transcript.isEmpty ? String(localized: "nothing yet") : trial.transcript.text
    }

    /// Ce que VoiceOver entend quand l'essai change d'état ; la fin d'un
    /// essai (réussi ou non) passe par `announcement`.
    nonisolated static func spoken(_ phase: MicTrialController.Phase) -> String? {
        switch phase {
        case .listening: String(localized: "Speak now.")
        case .finishing: String(localized: "The final text is coming.")
        case .failed(let message) where message != MicTrialController.noFinal: message
        default: nil
        }
    }

    private var status: String {
        switch trial.phase {
        case .idle: trial.results[trial.modelID ?? ""] == nil ? String(localized: "Ready.") : String(localized: "Trial finished.")
        case .preparing(let what): what
        case .listening: trial.speechHeard
            ? String(localized: "I can hear you. The trial will stop by itself at the end.")
            : String(localized: "Speak now.")
        case .finishing: String(localized: "The final text is coming…")
        case .failed(let message): message
        }
    }

    private var statusIsError: Bool {
        if case .failed = trial.phase { return true }
        return false
    }

    private var meterValue: String {
        guard trial.phase == .listening else { return String(localized: "microphone closed") }
        return trial.speechHeard ? String(localized: "voice heard") : String(localized: "waiting for your voice")
    }
}

/// Le niveau du micro : huit barres, hautes, qui suivent la voix (le
/// spectre de la dictée, `VoiceBars.levels`). Immobiles micro fermé.
struct MicLevelMeter: View {
    let feed: SpectrumFeed
    let live: Bool
    @State private var motion = BarMotion()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    static let width: CGFloat = 6
    static let gap: CGFloat = 5
    static let rest: CGFloat = 6
    static let tallest: CGFloat = 36

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 60, paused: !live)) { context in
            Canvas { canvas, size in
                let now = context.date.timeIntervalSinceReferenceDate
                if live { feed.advance(to: now) }
                let target = live ? VoiceBars.levels(feed.columns.last ?? []) : []
                let levels = motion.step(toward: target, at: now, smooth: !reduceMotion)
                var path = Path()
                for i in 0..<VoiceBars.count {
                    let h = Self.rest + (Self.tallest - Self.rest) * CGFloat(levels[i])
                    let x = CGFloat(i) * (Self.width + Self.gap)
                    path.addRoundedRect(in: CGRect(x: x, y: (size.height - h) / 2, width: Self.width, height: h),
                                        cornerSize: CGSize(width: Self.width / 2, height: Self.width / 2))
                }
                canvas.fill(path, with: .color(live ? Palette.ink : Palette.faint))
            }
        }
        .frame(width: CGFloat(VoiceBars.count) * Self.width + CGFloat(VoiceBars.count - 1) * Self.gap,
               height: Self.tallest)
    }
}

/// Les résultats de l'essai qui vient de finir, en grand.
struct MicTrialResultCard: View {
    let name: String
    let result: TrialResult

    var body: some View {
        let v = TrialFormat.values(result, size: { SizeFormat.bytes($0) })
        VStack(alignment: .leading, spacing: 12) {
            Text("Results for \(name)")
                .font(Typo.rowTitle)
                .foregroundStyle(Palette.ink)
            HStack(alignment: .top, spacing: 24) {
                metric(String(localized: "Accuracy"), result.freeText ? String(localized: "Free text") : v.precision,
                       String(localized: "correct words"))
                metric(String(localized: "Delay"), v.delay,
                       result.firstDelayFromSpeech
                           ? String(localized: "first text after speech starts, then final text after “Stop”")
                           : String(localized: "first text after the microphone opens, then final text after “Stop”"))
                metric(String(localized: "Throughput"), v.speed,
                       result.speed == nil ? String(localized: "not measured for macOS dictation")
                           : String(localized: "during dictation, re-reads included: seconds of speech per second of compute (not comparable to a speed measured in a single pass)"))
                metric(String(localized: "Memory"), v.memory,
                       result.memoryBytes == nil ? String(localized: "not available for macOS dictation")
                           : result.memoryMeasured ? String(localized: "used by the loaded model")
                           : String(localized: "estimated: the model's size"))
            }
        }
        .padding(Space.inset)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12).fill(Palette.control))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("essai-resultats")
    }

    private func metric(_ title: String, _ value: String, _ detail: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
            Text(value)
                .font(.system(size: 20, weight: .semibold))
                .monospacedDigit()
                .foregroundStyle(Palette.ink)
            Text(detail)
                .font(Typo.body)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Le dernier essai de chaque modèle, pour comparer : une ligne titrée par
/// modèle, chaque valeur avec son nom, qui passe à la ligne plutôt que de
/// se couper.
struct MicTrialHistory: View {
    let results: [String: TrialResult]
    let options: [(title: String, value: String)]

    /// Les valeurs d'un essai, nommées et en entier.
    nonisolated static func parts(_ r: TrialResult, size: (UInt64) -> String) -> [String] {
        let v = TrialFormat.values(r, size: size)
        var delay = String(localized: "Delay: \(v.delay)")
        if r.firstDelay != nil && !r.firstDelayFromSpeech { delay += String(localized: " (since the microphone opened)") }
        let memory = r.memoryBytes != nil && !r.memoryMeasured ? String(localized: "about \(v.memory)") : v.memory
        return [
            r.freeText ? String(localized: "Free text") : String(localized: "Accuracy: \(v.precision)"),
            delay,
            String(localized: "Throughput: \(v.speed)"),
            String(localized: "Memory: \(memory)"),
        ]
    }

    var body: some View {
        let shown = options.filter { results[$0.value] != nil }
        if !shown.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Text("Latest trials")
                    .font(Typo.rowTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.bottom, 6)
                Rule()
                ForEach(shown, id: \.value) { option in
                    if let result = results[option.value] { line(option.title, result) }
                    Rule()
                }
                Text("Delay: the first text, then the final text after “Stop”. Throughput: measured during dictation, re-reads included.")
                    .font(Typo.body)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("essai-historique")
        }
    }

    private func line(_ name: String, _ r: TrialResult) -> some View {
        let date = r.date.formatted(.dateTime.day().month(.wide).locale(UILanguage.numberLocale))
        let parts = Self.parts(r, size: { SizeFormat.bytes($0) })
        let spoken = parts.joined(separator: ", ")
        return VStack(alignment: .leading, spacing: 4) {
            Text("\(name), \(date)")
                .font(Typo.rowTitle)
                .foregroundStyle(Palette.ink)
                .fixedSize(horizontal: false, vertical: true)
            Text(parts.joined(separator: "   ·   "))
                .font(Typo.body)
                .monospacedDigit()
                .foregroundStyle(Palette.ink)
                .lineSpacing(4)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(String(localized: "\(name), \(date): \(spoken)"))
    }
}

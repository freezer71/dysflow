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
                Text("Essai du micro")
                    .font(Typo.sectionTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
                Text("Lisez la phrase à voix haute. Le texte reconnu s'affiche ici : il n'est saisi nulle part. "
                     + "Le modèle essayé ne change pas celui de la dictée.")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 20) {
                HStack(spacing: 8) {
                    Text("Modèle")
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                        .accessibilityHidden(true)
                    PopUpPicker(label: "Modèle de l'essai", identifier: "essai-modele", options: options,
                                selection: Binding(get: { selection }, set: { picked = $0 }))
                        .fixedSize()
                        .disabled(trial.isRunning)
                }
                Toggle(isOn: Binding(get: { trial.freeText }, set: { trial.freeText = $0 })) {
                    Text("Texte libre")
                        .font(Typo.rowTitle)
                        .foregroundStyle(Palette.ink)
                }
                .notionSwitch()
                .disabled(trial.isRunning)
                .accessibilityLabel("Texte libre : dire ce que vous voulez, sans précision")
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
        .padding(16)
        .overlay(alignment: .top) { Rule().padding(.horizontal, 16) }
        .onAppear { trial.refreshLanguage() }
        .onChange(of: language) { _, _ in trial.refreshLanguage() }
        // La section change ou la fenêtre se ferme : le micro se ferme.
        .onDisappear { trial.cancel() }
        .onChange(of: trial.announcement) { _, text in
            if let text { Announcement.post(text) }
        }
    }

    @ViewBuilder private var sentenceBox: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(trial.freeText ? "Dites ce que vous voulez" : "Phrase à lire")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
            if trial.freeText {
                Text("Parlez librement : la précision n'est pas calculée, les délais, le débit et la mémoire le sont.")
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
                    .accessibilityLabel("Phrase à lire : \(trial.sentence)")
                    .accessibilityIdentifier("essai-phrase")
                Button("Une autre phrase") { trial.nextSentence() }
                    .buttonStyle(.notionPlain)
                    .disabled(trial.isRunning)
                    .accessibilityIdentifier("essai-autre-phrase")
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12).fill(Palette.control))
    }

    private var controls: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 16) {
                MicLevelMeter(feed: trial.spectrum, live: trial.phase == .listening)
                    .accessibilityElement()
                    .accessibilityLabel("Niveau du micro")
                    .accessibilityValue(meterValue)
                    .accessibilityIdentifier("essai-niveau")
                Text(status)
                    .font(Typo.body)
                    .foregroundStyle(statusIsError ? ErrorStyle.color : Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("essai-etat")
                Spacer(minLength: 12)
                Button(trial.isRunning ? "Arrêter" : "Démarrer l'essai") {
                    if trial.isRunning { trial.stop() } else { trial.start(model: selection) }
                }
                .buttonStyle(.notionPrimary)
                .fixedSize()
                .disabled(!trial.isRunning && dictating || trial.phase == .finishing)
                .accessibilityLabel(trial.isRunning ? "Arrêter l'essai" : "Démarrer l'essai du micro")
                .accessibilityIdentifier("essai-demarrer")
            }
            if dictating && !trial.isRunning {
                Text(MicTrialController.dictationBusy)
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var heardBox: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Texte reconnu")
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .accessibilityHidden(true)
            Text(trial.transcript.isEmpty ? "Le texte apparaîtra ici pendant que vous parlez." : trial.transcript.text)
                .font(.system(size: 17))
                .foregroundStyle(trial.transcript.isEmpty ? Palette.muted : Palette.ink)
                .lineSpacing(5)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, minHeight: 48, alignment: .topLeading)
                .textSelection(.enabled)
                .accessibilityLabel("Texte reconnu : \(trial.transcript.isEmpty ? "rien pour l'instant" : trial.transcript.text)")
                .accessibilityIdentifier("essai-texte")
        }
    }

    private var status: String {
        switch trial.phase {
        case .idle: trial.results[trial.modelID ?? ""] == nil ? "Prêt." : "Essai terminé."
        case .preparing(let what): what
        case .listening: trial.speechHeard ? "Je vous entends. Appuyez sur « Arrêter » à la fin." : "Parlez maintenant."
        case .finishing: "Le texte final arrive…"
        case .failed(let message): message
        }
    }

    private var statusIsError: Bool {
        if case .failed = trial.phase { return true }
        return false
    }

    private var meterValue: String {
        guard trial.phase == .listening else { return "micro fermé" }
        return trial.speechHeard ? "voix entendue" : "en attente de la voix"
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
            Text("Résultats de \(name)")
                .font(Typo.rowTitle)
                .foregroundStyle(Palette.ink)
            HStack(alignment: .top, spacing: 24) {
                metric("Précision", result.freeText ? "Texte libre" : v.precision,
                       "mots justes")
                metric("Délai", v.delay,
                       result.firstDelayFromSpeech
                           ? "premier texte après le début de la parole, puis texte final après « Arrêter »"
                           : "premier texte après l'ouverture du micro, puis texte final après « Arrêter »")
                metric("Débit", v.speed,
                       result.speed == nil ? "non mesuré pour la dictée de macOS" : "secondes de parole par seconde de calcul")
                metric("Mémoire", v.memory,
                       result.memoryBytes == nil ? "non mesurée pour la dictée de macOS"
                           : result.memoryMeasured ? "occupée par le modèle chargé" : "estimée : le poids du modèle")
            }
        }
        .padding(16)
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
                .font(Typo.detail)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Le dernier essai de chaque modèle, pour comparer.
struct MicTrialHistory: View {
    let results: [String: TrialResult]
    let options: [(title: String, value: String)]

    private static let columns: [(String, CGFloat?)] = [
        ("Modèle", nil), ("Précision", 80), ("Délai", 130), ("Débit", 64), ("Mémoire", 80), ("Date", 90),
    ]

    var body: some View {
        let shown = options.filter { results[$0.value] != nil }
        if !shown.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Text("Derniers essais")
                    .font(Typo.rowTitle)
                    .foregroundStyle(Palette.ink)
                    .padding(.bottom, 6)
                HStack(spacing: 12) {
                    ForEach(Self.columns, id: \.0) { title, width in cell(title, width) }
                }
                .font(Typo.caption)
                .foregroundStyle(Palette.muted)
                .padding(.vertical, 6)
                .accessibilityHidden(true)
                Rule()
                ForEach(shown, id: \.value) { option in
                    if let result = results[option.value] { line(option.title, result) }
                    Rule()
                }
                Text("Délai : le premier texte, puis le texte final. Une étoile : compté depuis l'ouverture du micro, "
                     + "le début de la parole n'ayant pas été repéré.")
                    .font(Typo.detail)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("essai-historique")
        }
    }

    private func line(_ name: String, _ r: TrialResult) -> some View {
        let v = TrialFormat.values(r, size: { SizeFormat.bytes($0) })
        let delay = r.firstDelay != nil && !r.firstDelayFromSpeech ? v.delay + " *" : v.delay
        let date = r.date.formatted(.dateTime.day().month(.abbreviated).locale(Locale(identifier: "fr_FR")))
        let memory = r.memoryBytes != nil && !r.memoryMeasured ? "≈ " + v.memory : v.memory
        let values = [name, r.freeText ? "libre" : v.precision, delay, v.speed, memory, date]
        return HStack(spacing: 12) {
            ForEach(Array(zip(Self.columns, values).enumerated()), id: \.offset) { _, pair in
                cell(pair.1, pair.0.1)
            }
        }
        .font(Typo.body)
        .monospacedDigit()
        .foregroundStyle(Palette.ink)
        .padding(.vertical, 8)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(TrialFormat.announcement(r, model: name, size: { SizeFormat.bytes($0) })
            .replacingOccurrences(of: "Essai de \(name) terminé.", with: "\(name), le \(date) :"))
    }

    private func cell(_ text: String, _ width: CGFloat?) -> some View {
        Group {
            if let width {
                Text(text).frame(width: width, alignment: .leading)
            } else {
                Text(text).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .lineLimit(1)
    }
}

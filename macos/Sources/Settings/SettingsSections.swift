import AVFoundation
import KeyboardShortcuts
import SwiftUI

/// Voix : la table des modèles de lecture, une voix pour toutes les langues
/// et son « Écouter », la place occupée, la vitesse, la sortie audio.
struct VoiceSection: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    /// Les modèles (lecture et dictée), tels que le moteur les donne.
    let rows: [VoiceModel]
    /// Octets occupés par les modèles ; libres sur le disque, si on le sait.
    let used: UInt64
    let free: UInt64?
    /// Relit les modèles et les réglages après une action de la table.
    let onChange: () async -> Void
    /// L'erreur et la suppression en attente de la table, gardées par la fenêtre.
    let table: ModelsTableState
    /// La qualité des voix de macOS de la liste, lue par la fenêtre quand la
    /// liste change (pas à chaque retour sur la section).
    let macQuality: [String: Int]
    /// La bibliothèque des voix est ouverte, en feuille sur la fenêtre.
    @State private var libraryShown = false

    var body: some View {
        SettingsGroup(title: String(localized: "Voice")) {
            ModelsTable(rows: rows.filter { $0.kind == "speech" }, model: model,
                        fallback: String(localized: "Reading will switch to the macOS voice."), onChange: onChange, state: table)
            if !store.capabilities.voices.isEmpty && !Self.suggestsModels(model.snapshot.voice) {
                SettingRow(title: String(localized: "Voice"),
                           detail: String(localized: "Reads all the text, whatever its language")) {
                    HStack(spacing: 8) {
                        PopUpPicker(label: String(localized: "Voice"), identifier: "voix-unique",
                                    options: Self.voiceOptions(store.capabilities, quality: macQuality),
                                    selection: Binding(get: { Self.voice(in: store.values, store.capabilities) },
                                                       set: { store.setFamilyVoice($0) }))
                            .frame(width: 260)
                        Button {
                            let voice = Self.voice(in: store.values, store.capabilities)
                            Task { await store.listen(language: Self.language(of: voice, store.capabilities)) }
                        } label: {
                            HStack(spacing: 5) {
                                Image(systemName: "play.fill").font(.system(size: 10))
                                Text("Listen")
                            }
                        }
                        .buttonStyle(.notion)
                        .fixedSize()
                        .accessibilityLabel("Listen to the voice")
                        .accessibilityIdentifier("ecouter-voix")
                    }
                }
                if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                    HStack {
                        Text(line)
                            .font(Typo.detail)
                            .foregroundStyle(Palette.muted)
                            .accessibilityIdentifier("etat-ecoute")
                        Spacer()
                        Button("Stop") { Task { await store.stopListening() } }
                            .buttonStyle(.notionPlain)
                            .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                            .accessibilityLabel("Stop listening")
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, Space.inset)
                }
                if store.capabilities.family == "system" {
                    Link("Add voices in System Settings…",
                         destination: URL(string: "x-apple.systempreferences:com.apple.preference.universalaccess")!)
                        .font(Typo.detail)
                        .padding(.horizontal, Space.inset)
                        .padding(.bottom, 8)
                }
            }
            SettingRow(title: String(localized: "Voice library"),
                       detail: String(localized: "All installed voices, to try on your text before choosing")) {
                Button("Browse voices…") { libraryShown = true }
                    .buttonStyle(.notion)
                    .accessibilityIdentifier("parcourir-voix")
            }
            Text(ModelTable.footer(used: used, free: free))
                .font(Typo.detail)
                .foregroundStyle(Palette.muted)
                .padding(.horizontal, Space.inset)
                .padding(.vertical, 10)
                .accessibilityIdentifier("place-modeles")
            SettingRow(title: String(localized: "Speed"),
                       detail: String(localized: "In the reader: + and −, or the speed in the bar")) {
                Slider(value: Binding(get: { store.values.speed },
                                      set: { store.set(\.speed, SettingFormat.rounded($0)) }),
                       in: 0.5...3, step: 0.05) {
                    Text("Speed: \(SettingFormat.decimal(store.values.speed))×")
                }
                .notionSlider()
                .accessibilityLabel("Reading speed")
                .accessibilityIdentifier("vitesse")
                .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) times")
                ValueText("\(SettingFormat.decimal(store.values.speed))×")
            }
            SettingRow(title: String(localized: "Audio output"),
                       detail: String(localized: "Sound plays only there: if it is unplugged, reading pauses")) {
                PopUpPicker(label: String(localized: "Audio output"), identifier: "sortie-audio",
                            options: AudioOutputChoices.choices(model.outputs, chosen: model.outputDevice)
                                .map { ($0.title, $0.value) },
                            selection: Binding(get: { model.outputDevice }, set: { model.chooseOutput($0) }))
                    .frame(width: 240)
            }
        }
        // L'échec d'un aperçu se dit à VoiceOver depuis la fenêtre
        // (`ReglagesView`), quelle que soit la section affichée.
        .sheet(isPresented: $libraryShown) {
            let voice = Self.voice(in: store.values, store.capabilities)
            VoiceLibrarySheet(model: model,
                              sampleText: previewSampleText(language: Self.language(of: voice, store.capabilities)),
                              speed: store.values.speed) { entry in
                try await useLibraryVoice(entry)
            }
        }
    }

    /// « Utiliser » dans la bibliothèque : le modèle de la voix d'abord (s'il
    /// ne lit pas déjà), puis la voix, écrite tout de suite.
    private func useLibraryVoice(_ entry: VoiceLibraryEntry) async throws {
        if !entry.modelActive {
            try await model.useModel(entry.modelID)
            await onChange()
        }
        store.setFamilyVoice(entry.voiceID)
        await store.flush()
    }

    /// Voix de repli ou voix muette.
    static func suggestsModels(_ state: VoiceState) -> Bool {
        switch state {
        case .fallback, .unavailable: true
        default: false
        }
    }

    /// Avec la voix de macOS, « Voix par défaut de macOS » (valeur vide) :
    /// celle que `say` prend quand aucune n'est choisie.
    static let macDefaultVoice = String(localized: "macOS default voice")

    /// Les voix du menu : celles du modèle ; la voix de macOS ajoute en tête
    /// sa voix par défaut, puis ses voix Premium et Améliorées.
    static func voiceOptions(_ c: ModelCapabilities, quality: [String: Int] = [:])
        -> [(title: String, value: String)] {
        guard c.family == "system" else { return c.voices.map { (title: $0.label, value: $0.id) } }
        let byID = Dictionary(c.voices.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let listed = ordered(c.voices.map(\.id), quality: quality).compactMap { id in
            byID[id].map { (title: $0.label, value: $0.id) }
        }
        return [(title: macDefaultVoice, value: "")] + listed
    }

    /// Les voix Premium (3), puis Améliorées (2), puis les autres ; dans
    /// l'ordre reçu à qualité égale.
    nonisolated static func ordered(_ names: [String], quality: [String: Int]) -> [String] {
        names.enumerated()
            .sorted { a, b in
                let qa = quality[a.element] ?? 1, qb = quality[b.element] ?? 1
                return qa != qb ? qa > qb : a.offset < b.offset
            }
            .map(\.element)
    }

    /// Une voix de macOS vue par AVFoundation : son nom, sa qualité
    /// (3 Premium, 2 Améliorée, 1 sinon) et son genre (« femme », « homme »,
    /// vide s'il n'est pas dit : les mots du moteur, jamais affichés tels quels).
    struct MacVoice: Equatable {
        let name: String
        let quality: Int
        var gender = ""
    }

    /// Les voix installées, lues une fois par liste de voix.
    static func installedMacVoices() -> [MacVoice] {
        AVSpeechSynthesisVoice.speechVoices().map { voice in
            let level = switch voice.quality {
            case .premium: 3
            case .enhanced: 2
            default: 1
            }
            let gender = switch voice.gender {
            case .female: "femme"   // xcstrings: garder
            case .male: "homme"   // xcstrings: garder
            default: ""
            }
            return MacVoice(name: voice.name, quality: level, gender: gender)
        }
    }

    /// Le genre de chaque voix listée par `say` (`ids`), d'après la voix
    /// d'AVFoundation du même nom de base (« Zoe (Premium) » : celui de
    /// Zoe). Inconnu : absent.
    nonisolated static func genders(of ids: [String], voices: [MacVoice]) -> [String: String] {
        var result: [String: String] = [:]
        for id in ids {
            let base = split(id).base
            if let gender = voices.first(where: { split($0.name).base == base && !$0.gender.isEmpty })?.gender {
                result[id] = gender
            }
        }
        return result
    }

    /// La qualité de chaque voix listée par `say` (`ids`), d'après les voix
    /// d'AVFoundation. `say` écrit la qualité après le nom : « Zoe (Premium) »,
    /// « Audrey (Améliorée) », « Thomas (Amélioré) », « Ava (Enhanced) ». Une
    /// voix ainsi marquée vaut sa qualité si AVFoundation a une voix de ce nom
    /// et de cette qualité. Un nom seul qui a un jumeau marqué dans la liste
    /// est la voix compacte : 1. Sans voix AVFoundation de ce nom : 1.
    nonisolated static func quality(of ids: [String], voices: [MacVoice]) -> [String: Int] {
        let parsed = ids.map { (id: $0, name: split($0)) }
        let twinned = Set(parsed.filter { $0.name.level != nil }.map(\.name.base))
        var result: [String: Int] = [:]
        for (id, name) in parsed {
            let matches = voices.filter { split($0.name).base == name.base }
            let level: Int
            if let marked = name.level {
                level = matches.contains { $0.quality == marked } ? marked : 1
            } else if twinned.contains(name.base) {
                level = 1
            } else {
                level = matches.map(\.quality).max() ?? 1
            }
            result[id] = level
        }
        return result
    }

    /// « Zoe (Premium) » : le nom de base (en minuscules) et la qualité que
    /// dit la parenthèse, `nil` si elle n'en dit pas.
    nonisolated private static func split(_ name: String) -> (base: String, level: Int?) {
        guard name.hasSuffix(")"), let open = name.range(of: " (", options: .backwards) else {
            return (name.trimmingCharacters(in: .whitespaces).lowercased(), nil)
        }
        let base = String(name[..<open.lowerBound]).trimmingCharacters(in: .whitespaces).lowercased()
        let tag = name[open.upperBound...].dropLast().lowercased()
        switch tag {
        case "premium": return (base, 3)
        case "enhanced", "améliorée", "amélioré": return (base, 2)   // xcstrings: garder (étiquettes de `say`)
        default: return (name.lowercased(), nil)
        }
    }

    /// La voix en place. Kokoro : la française (une voix pour tout), absente
    /// du modèle, sa première. Voix de macOS : `voice`, vide ou absente de la
    /// liste, la voix par défaut ("") — celle qui parle vraiment.
    static func voice(in values: SettingsView, _ c: ModelCapabilities) -> String {
        if c.family == "system" {
            return c.voices.contains(where: { $0.id == values.voice }) ? values.voice : ""
        }
        let chosen = values.voicesByLanguage.first(where: { $0.language == "fr" })?.voice
        return c.voices.first(where: { $0.id == chosen })?.id ?? c.voices.first?.id ?? ""
    }

    /// L'aperçu se dit dans la langue de la voix.
    static func language(of voice: String, _ c: ModelCapabilities) -> String {
        c.voices.first(where: { $0.id == voice })?.language ?? "fr"
    }

    static func previewLine(_ preview: PreviewView, refusal: String?) -> String? {
        if let refusal { return refusal }
        if let error = preview.error { return error }
        if preview.pending { return String(localized: "Preparing the preview…") }
        if preview.playing { return String(localized: "Listening…") }
        return nil
    }
}

/// Lecture : police, taille, interlignage, règle de lecture.
struct ReadingSection: View {
    @Bindable var store: SettingsStore

    static let fontOptions: [(title: String, value: String)] = ReadingFont.allCases.map { ($0.label, $0.rawValue) }

    /// La police telle qu'elle s'affiche : une valeur inconnue, Literata.
    static func shownFont(_ stored: String) -> String {
        ReadingFont(rawValue: stored)?.rawValue ?? ReadingFont.literata.rawValue
    }

    var body: some View {
        SettingsGroup(title: String(localized: "Reading")) {
            SettingRow(title: String(localized: "Font")) {
                PopUpPicker(label: String(localized: "Reading font"), identifier: "police-lecture", options: Self.fontOptions,
                            selection: Binding(get: { Self.shownFont(store.values.readingFont) },
                                               set: { store.set(\.readingFont, $0) }))
                    .fixedSize()
            }
            SettingRow(title: String(localized: "Size")) {
                Slider(value: store.binding(\.readingSize), in: 14...40, step: 1) {
                    Text("Size: \(Int(store.values.readingSize)) pt")
                }
                .notionSlider()
                .accessibilityLabel("Text size")
                .accessibilityIdentifier("taille-texte")
                .accessibilityValue("\(Int(store.values.readingSize)) points")
                ValueText("\(Int(store.values.readingSize)) pt")
            }
            SettingRow(title: String(localized: "Line spacing"), detail: String(localized: "The space between two lines")) {
                ValueText("\(store.values.lineSpacing)", width: 20)
                Stepper("Line spacing: \(store.values.lineSpacing)",
                        value: Binding(get: { Int(store.values.lineSpacing) },
                                       set: { store.set(\.lineSpacing, UInt32(max(0, $0))) }),
                        in: 0...3)
                    .labelsHidden()
                    .accessibilityLabel("Line spacing")
                    .accessibilityIdentifier("interlignage")
                    .accessibilityValue("\(store.values.lineSpacing)")
            }
            SettingRow(title: String(localized: "Reading ruler"),
                       detail: String(localized: "A band under the line being read, so you do not lose it")) {
                Toggle("Reading ruler", isOn: store.binding(\.readingRuler))
                    .notionSwitch()
                    .accessibilityIdentifier("regle-lecture")
            }
        }
    }
}

/// Raccourcis : les deux raccourcis globaux. Un raccourci déjà pris est
/// signalé par l'enregistreur lui-même.
struct ShortcutsSection: View {
    struct Row {
        let title: String
        let detail: String
        let name: KeyboardShortcuts.Name
        let identifier: String
    }

    static let rows = [
        Row(title: String(localized: "Read the selection"),
            detail: String(localized: "The selected text, or the clipboard if nothing is selected"),
            name: .readSelection, identifier: "raccourci-lecture-selection"),
        Row(title: String(localized: "Show / hide the reader"), detail: String(localized: "At its last size"),
            name: .showPanel, identifier: "raccourci-afficher"),
    ]

    static let voiceOverNote = String(localized: "⌃⌥ is also the VoiceOver key: with VoiceOver on, choose other keys.")

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SettingsGroup(title: String(localized: "Shortcuts")) {
                ForEach(Self.rows, id: \.identifier) { row in
                    SettingRow(title: row.title, detail: row.detail) {
                        ShortcutRecorder(target: .shortcut(row.name), title: row.title, identifier: row.identifier)
                    }
                }
                if Dictation.isSupported {
                    DictationShortcutRow(identifier: "raccourci-dictee-general")
                }
            }
            Callout(symbol: "info.circle", text: Self.voiceOverNote, identifier: "raccourcis-note-voiceover")
        }
    }
}

/// Le raccourci de la dictée, dans la section Dictée et dans Raccourcis :
/// une touche de modification seule (un appui bref) ou une combinaison.
struct DictationShortcutRow: View {
    let identifier: String

    var body: some View {
        SettingRow(title: String(localized: "Dictate"),
                   detail: String(localized: "One press opens dictation, a second types the text where you write; Esc cancels. A single key (right Option…) needs Accessibility")) {
            ShortcutRecorder(target: .dictation, title: String(localized: "Dictation shortcut"), identifier: identifier)
        }
    }
}

/// La dictée (macOS 26) : ses modèles, l'essai du micro, son raccourci et sa langue.
struct DictationSection: View {
    let model: ReaderModel
    /// Les modèles (lecture et dictée), tels que le moteur les donne.
    let rows: [VoiceModel]
    /// Relit les modèles et les réglages après une action de la table.
    let onChange: () async -> Void
    /// L'erreur et la suppression en attente de la table, gardées par la fenêtre.
    let table: ModelsTableState
    @AppStorage(DictationLanguage.defaultsKey) private var language = DictationLanguage.stored().rawValue
    @AppStorage(DictationModel.defaultsKey) private var chosen = DictationModel.stored()
    @AppStorage(DictationStats.defaultsKey) private var showStats = DictationStats.shown()
    /// La bibliothèque des modèles est ouverte, en feuille sur la fenêtre.
    @State private var libraryShown = false

    /// La ligne de la dictée de macOS : jamais téléchargée ni supprimée.
    static let systemID = "system-dictation"
    static let fallback = String(localized: "Dictation will switch to macOS dictation.")

    /// La dictée de macOS en tête, puis les modèles de dictée. La ligne
    /// utilisée vient du choix des réglages (`dictationModel`), jamais du
    /// moteur Rust.
    nonisolated static func rows(_ all: [VoiceModel], chosen: String) -> [VoiceModel] {
        let system = VoiceModel(id: systemID, name: DictationModel.label(DictationModel.system), bytes: 0,
                                installed: true, active: false, recommended: false,
                                summary: String(localized: "On this Mac, no download."), family: "system",
                                precision: "", languagesLabel: String(localized: "French, English"), languages: ["fr", "en"],
                                pace: "", voicesLabel: "", licenses: "", kind: "dictation", diskBytes: 0,
                                system: true, hasFiles: false)
        return ([system] + all.filter { $0.kind == "dictation" }).map { row in
            var row = row
            row.active = isUsed(row, chosen: chosen)
            return row
        }
    }

    nonisolated static func isUsed(_ row: VoiceModel, chosen: String) -> Bool {
        row.id == systemID ? chosen == DictationModel.system : row.id == chosen
    }

    /// Le repli sur la dictée de macOS, s'il concerne le modèle choisi.
    nonisolated static func notice(_ notice: String?, chosen: String) -> String? {
        guard let notice, chosen != DictationModel.system,
              notice.hasPrefix(DictationModel.label(chosen)) else { return nil }
        return notice
    }

    static let options = DictationLanguage.allCases.map { (title: $0.label, value: $0.rawValue) }

    /// Pendant un essai du micro, la table ne change rien : le modèle essayé
    /// ne doit pas être libéré ni supprimé sous ses pieds.
    static let lockedByTrial = String(localized: "A microphone trial is running: the model table waits until it is done.")

    var body: some View {
        let trialRunning = AppDelegate.shared?.micTrial?.isRunning ?? false
        let all = Self.rows(rows, chosen: chosen)
        SettingsGroup(title: String(localized: "Dictation")) {
            // La table ne garde que ce qui est là ou en route ; le reste est
            // dans la bibliothèque des modèles.
            ModelsTable(rows: ModelTable.shortlist(all, download: model.snapshot.download), model: model,
                        fallback: Self.fallback,
                        isUsed: { Self.isUsed($0, chosen: chosen) },
                        use: { row in use(row) },
                        removed: { row in removed(row) },
                        onChange: onChange, state: table)
                .disabled(trialRunning)
                // Le modèle choisi change (un autre, la dictée de macOS, ou le
                // sien supprimé) : l'ancien quitte la mémoire, son repli s'efface.
                .onChange(of: chosen) { old, new in
                    guard old != new else { return }
                    // L'essai lit par le même coureur : il s'arrête avant la libération.
                    AppDelegate.shared?.micTrial?.cancel()
                    AppDelegate.shared?.dictationRouter?.releaseModel()
                }
            if trialRunning {
                Text(Self.lockedByTrial)
                    .font(Typo.body)
                    .foregroundStyle(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Space.inset)
                    .padding(.vertical, 8)
                    .accessibilityIdentifier("dictee-table-verrouillee")
            }
            // Le modèle choisi n'a pas pu servir : la dictée de macOS l'a
            // remplacé. Le message en entier, sous la table.
            if let notice = Self.notice(AppDelegate.shared?.dictationRouter?.fallbackNotice, chosen: chosen) {
                Text(notice)
                    .font(Typo.detail)
                    .foregroundStyle(ErrorStyle.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Space.inset)
                    .padding(.vertical, 8)
                    .accessibilityIdentifier("dictee-repli")
            }
            SettingRow(title: String(localized: "Model library"),
                       detail: String(localized: "All dictation models, to search, download and choose")) {
                Button("Browse models…") { libraryShown = true }
                    .buttonStyle(.notion)
                    .accessibilityIdentifier("parcourir-modeles")
            }
            // L'essai du micro : un modèle essayé sans changer celui de la dictée.
            if let trial = AppDelegate.shared?.micTrial {
                MicTrialBlock(trial: trial, rows: rows, chosen: chosen,
                              dictating: AppDelegate.shared?.dictation?.holdsMicrophone ?? false)
            }
            DictationShortcutRow(identifier: "raccourci-dictee")
            SettingRow(title: String(localized: "Language"),
                       detail: String(localized: "Recognized on your Mac: nothing leaves it. Parakeet and Voxtral detect it on their own")) {
                PopUpPicker(label: String(localized: "Dictation language"), identifier: "langue-dictee",
                            options: Self.options, selection: $language)
                    .fixedSize()
            }
            SettingRow(title: String(localized: "Delay and memory"),
                       detail: String(localized: "In the dictation window: the text delay and the memory the model uses")) {
                Toggle("Delay and memory", isOn: $showStats)
                    .notionSwitch()
                    .accessibilityIdentifier("mesures-dictee-reglage")
            }
        }
        .sheet(isPresented: $libraryShown) {
            ModelLibrarySheet(model: model, rows: all, locked: trialRunning,
                              onUse: { row in use(row) }, removed: { row in removed(row) },
                              onChange: onChange)
        }
    }

    /// Choisir une ligne (table ou bibliothèque) : le changement de `chosen`
    /// arrête l'essai et libère l'ancien modèle (`onChange(of: chosen)`).
    private func use(_ row: VoiceModel) {
        chosen = row.id == Self.systemID ? DictationModel.system : row.id
    }

    /// Après la suppression d'un modèle, depuis la table ou la bibliothèque.
    private func removed(_ row: VoiceModel) {
        // Un essai en route s'arrête d'abord : il ne réenregistrerait rien
        // après l'oubli.
        AppDelegate.shared?.micTrial?.cancel()
        if row.id == chosen { chosen = DictationModel.system }
        // Ses derniers essais partent avec lui.
        AppDelegate.shared?.micTrial?.forget(row.id)
    }
}

/// Les autorisations de macOS : leur état, et de quoi les demander. macOS
/// ne prévient pas d'un changement : l'état se relit chaque seconde, tant
/// que la fenêtre est ouverte.
struct PermissionsSection: View {
    @State private var states: [Permission: Permission.State] = [:]
    /// La fenêtre des réglages se voit (ni fermée, ni couverte).
    @State private var visible = false

    var body: some View {
        SettingsGroup(title: String(localized: "Permissions")) {
            ForEach(Permission.relevant, id: \.self) { permission in
                SettingRow(title: permission.title, detail: permission.detail) {
                    if states[permission] == .granted {
                        Tag(text: String(localized: "Allowed"), tint: .green)
                            .accessibilityIdentifier(permission.identifier)
                    } else {
                        Button(states[permission] == .denied ? "Open Settings…" : "Allow…") {
                            Task {
                                await permission.request()
                                reload()
                            }
                        }
                        .buttonStyle(.notionPrimary)
                        .accessibilityLabel("Allow: \(permission.title)")
                        .accessibilityIdentifier(permission.identifier)
                    }
                }
            }
        }
        // Relu à la seconde tant que la fenêtre se voit : la scène Settings
        // garde sa vue en vie fenêtre fermée, la tâche n'y serait pas annulée.
        // (Posé après la mise à jour de la vue : la fenêtre le dit pendant.)
        .background(WindowVisibility(onVisible: { shown in
            DispatchQueue.main.async { if shown != visible { visible = shown } }
        }, onClose: {}))
        .task(id: visible) {
            guard visible else { return }
            while !Task.isCancelled {
                reload()
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    private func reload() {
        var next: [Permission: Permission.State] = [:]
        for permission in Permission.relevant { next[permission] = permission.state }
        if next != states { states = next }
    }
}

/// Ouvrir sillage à la connexion.
struct StartupSection: View {
    @State private var login = false

    var body: some View {
        SettingsGroup(title: String(localized: "Startup")) {
            SettingRow(title: String(localized: "Open sillage at login"),
                       detail: String(localized: "In the menu bar, ready to read")) {
                Toggle("Open sillage at login", isOn: $login)
                    .notionSwitch()
                    .onAppear { login = LoginItem.isEnabled }
                    .onChange(of: login) { _, on in
                        // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                        if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                    }
                    .accessibilityLabel("Open sillage at login")
                    .accessibilityIdentifier("ouvrir-connexion")
            }
        }
    }
}

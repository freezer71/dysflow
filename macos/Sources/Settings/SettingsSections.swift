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
        SettingsGroup(title: "Voix") {
            ModelsTable(rows: rows.filter { $0.kind == "speech" }, model: model,
                        fallback: "La lecture passera à la voix de macOS.", onChange: onChange, state: table)
            if !store.capabilities.voices.isEmpty && !Self.suggestsModels(model.snapshot.voice) {
                SettingRow(title: "Voix", detail: "Lit tout le texte, quelle que soit sa langue") {
                    HStack(spacing: 8) {
                        PopUpPicker(label: "Voix", identifier: "voix-unique",
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
                                Text("Écouter")
                            }
                        }
                        .buttonStyle(.notion)
                        .fixedSize()
                        .accessibilityLabel("Écouter la voix")
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
                        Button("Arrêter") { Task { await store.stopListening() } }
                            .buttonStyle(.notionPlain)
                            .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                            .accessibilityLabel("Arrêter l'écoute")
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 16)
                }
                if store.capabilities.family == "system" {
                    Link("Ajouter des voix dans Réglages Système…",
                         destination: URL(string: "x-apple.systempreferences:com.apple.preference.universalaccess")!)
                        .font(Typo.detail)
                        .padding(.bottom, 4)
                }
            }
            SettingRow(title: "Bibliothèque des voix",
                       detail: "Toutes les voix installées, à écouter sur votre texte avant de choisir") {
                Button("Parcourir les voix…") { libraryShown = true }
                    .buttonStyle(.notion)
                    .accessibilityIdentifier("parcourir-voix")
            }
            Text(ModelTable.footer(used: used, free: free))
                .font(Typo.detail)
                .foregroundStyle(Palette.muted)
                .padding(.vertical, 4)
                .accessibilityIdentifier("place-modeles")
            SettingRow(title: "Vitesse", detail: "Dans le lecteur : + et −, ou la vitesse de la barre") {
                Slider(value: Binding(get: { store.values.speed },
                                      set: { store.set(\.speed, SettingFormat.rounded($0)) }),
                       in: 0.5...3, step: 0.05) {
                    Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                }
                .notionSlider()
                .accessibilityLabel("Vitesse de lecture")
                .accessibilityIdentifier("vitesse")
                .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
                ValueText("\(SettingFormat.decimal(store.values.speed))×")
            }
            SettingRow(title: "Sortie audio",
                       detail: "Le son ne sort que par là : débranchée, la lecture se met en pause") {
                PopUpPicker(label: "Sortie audio", identifier: "sortie-audio",
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
    static let macDefaultVoice = "Voix par défaut de macOS"

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
    /// vide s'il n'est pas dit).
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
            case .female: "femme"
            case .male: "homme"
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
        case "enhanced", "améliorée", "amélioré": return (base, 2)
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
        if preview.pending { return "Synthèse de l'aperçu…" }
        if preview.playing { return "Écoute…" }
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
        SettingsGroup(title: "Lecture") {
            SettingRow(title: "Police") {
                PopUpPicker(label: "Police de lecture", identifier: "police-lecture", options: Self.fontOptions,
                            selection: Binding(get: { Self.shownFont(store.values.readingFont) },
                                               set: { store.set(\.readingFont, $0) }))
                    .fixedSize()
            }
            SettingRow(title: "Taille") {
                Slider(value: store.binding(\.readingSize), in: 14...40, step: 1) {
                    Text("Taille : \(Int(store.values.readingSize)) pt")
                }
                .notionSlider()
                .accessibilityLabel("Taille du texte")
                .accessibilityIdentifier("taille-texte")
                .accessibilityValue("\(Int(store.values.readingSize)) points")
                ValueText("\(Int(store.values.readingSize)) pt")
            }
            SettingRow(title: "Interlignage", detail: "L'air entre deux lignes") {
                ValueText("\(store.values.lineSpacing)", width: 20)
                Stepper("Interlignage : \(store.values.lineSpacing)",
                        value: Binding(get: { Int(store.values.lineSpacing) },
                                       set: { store.set(\.lineSpacing, UInt32(max(0, $0))) }),
                        in: 0...3)
                    .labelsHidden()
                    .accessibilityLabel("Interlignage")
                    .accessibilityIdentifier("interlignage")
                    .accessibilityValue("\(store.values.lineSpacing)")
            }
            SettingRow(title: "Règle de lecture", detail: "Une bande sous la ligne lue, pour ne pas la perdre") {
                Toggle("Règle de lecture", isOn: store.binding(\.readingRuler))
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
        Row(title: "Lire la sélection", detail: "Le texte sélectionné, sinon le presse-papiers",
            name: .readSelection, identifier: "raccourci-lecture-selection"),
        Row(title: "Afficher / masquer le lecteur", detail: "Dans sa dernière taille",
            name: .showPanel, identifier: "raccourci-afficher"),
    ]

    static let voiceOverNote = "⌃⌥ est aussi la touche de VoiceOver : avec VoiceOver actif, choisissez d'autres touches."

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SettingsGroup(title: "Raccourcis") {
                ForEach(Self.rows, id: \.identifier) { row in
                    SettingRow(title: row.title, detail: row.detail) {
                        KeyboardShortcuts.Recorder(for: row.name)
                            .frame(width: 150)
                            .accessibilityLabel(row.title)
                            .accessibilityIdentifier(row.identifier)
                    }
                }
            }
            Callout(symbol: "info.circle", text: Self.voiceOverNote, identifier: "raccourcis-note-voiceover")
        }
    }
}

/// La dictée (macOS 26) : ses modèles, son raccourci et sa langue.
struct DictationSection: View {
    let model: ReaderModel
    /// Les modèles (lecture et dictée), tels que le moteur les donne.
    let rows: [VoiceModel]
    /// Relit les modèles et les réglages après une action de la table.
    let onChange: () async -> Void
    /// L'erreur et la suppression en attente de la table, gardées par la fenêtre.
    let table: ModelsTableState
    @AppStorage(DictationLanguage.defaultsKey) private var language = DictationLanguage.stored().rawValue
    @AppStorage(DictationModifier.defaultsKey) private var modifier = DictationModifier.stored().rawValue
    @AppStorage(DictationModel.defaultsKey) private var chosen = DictationModel.stored()

    /// La ligne de la dictée de macOS : jamais téléchargée ni supprimée.
    static let systemID = "system-dictation"
    static let fallback = "La dictée passera à celle de macOS."

    /// La dictée de macOS en tête, puis les modèles de dictée. La ligne
    /// utilisée vient du choix des réglages (`dictationModel`), jamais du
    /// moteur Rust.
    nonisolated static func rows(_ all: [VoiceModel], chosen: String) -> [VoiceModel] {
        let system = VoiceModel(id: systemID, name: DictationModel.label(DictationModel.system), bytes: 0,
                                installed: true, active: false, recommended: false,
                                summary: "Sur ce Mac, sans téléchargement.", family: "system",
                                precision: "", languagesLabel: "Français, anglais", languages: ["fr", "en"],
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
    static let modifierOptions = DictationModifier.allCases.map { (title: $0.label, value: $0.rawValue) }

    var body: some View {
        SettingsGroup(title: "Dictée") {
            ModelsTable(rows: Self.rows(rows, chosen: chosen), model: model, fallback: Self.fallback,
                        isUsed: { Self.isUsed($0, chosen: chosen) },
                        use: { row in chosen = row.id == Self.systemID ? DictationModel.system : row.id },
                        removed: { row in if row.id == chosen { chosen = DictationModel.system } },
                        onChange: onChange, state: table)
                // Le modèle choisi change (un autre, la dictée de macOS, ou le
                // sien supprimé) : l'ancien quitte la mémoire, son repli s'efface.
                .onChange(of: chosen) { old, new in
                    if old != new { AppDelegate.shared?.dictationRouter?.releaseModel() }
                }
            // Le modèle choisi n'a pas pu servir : la dictée de macOS l'a
            // remplacé. Le message en entier, sous la table.
            if let notice = Self.notice(AppDelegate.shared?.dictationRouter?.fallbackNotice, chosen: chosen) {
                Text(notice)
                    .font(Typo.detail)
                    .foregroundStyle(ErrorStyle.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.vertical, 4)
                    .accessibilityIdentifier("dictee-repli")
            }
            SettingRow(title: "Dicter",
                       detail: "Un appui ouvre la dictée, un second saisit le texte là où vous écrivez ; Échap annule") {
                KeyboardShortcuts.Recorder(for: .dictate)
                    .frame(width: 150)
                    .accessibilityLabel("Dicter")
                    .accessibilityIdentifier("raccourci-dictee")
            }
            SettingRow(title: "Touche seule",
                       detail: "Un appui bref, sans autre touche : comme le raccourci (il faut l'Accessibilité)") {
                PopUpPicker(label: "Touche seule pour dicter", identifier: "touche-dictee",
                            options: Self.modifierOptions, selection: $modifier)
                    .fixedSize()
            }
            SettingRow(title: "Langue",
                       detail: "Reconnue sur votre Mac : rien n'en sort. Parakeet et Voxtral la reconnaissent seuls") {
                PopUpPicker(label: "Langue de la dictée", identifier: "langue-dictee",
                            options: Self.options, selection: $language)
                    .fixedSize()
            }
        }
    }
}

/// Les autorisations de macOS : leur état, et de quoi les demander. macOS
/// ne prévient pas d'un changement : l'état se relit chaque seconde, tant
/// que la fenêtre est ouverte.
struct PermissionsSection: View {
    @State private var states: [Permission: Permission.State] = [:]

    var body: some View {
        SettingsGroup(title: "Autorisations") {
            ForEach(Permission.relevant, id: \.self) { permission in
                SettingRow(title: permission.title, detail: permission.detail) {
                    if states[permission] == .granted {
                        Tag(text: "Autorisé", tint: .green)
                            .accessibilityIdentifier(permission.identifier)
                    } else {
                        Button(states[permission] == .denied ? "Ouvrir les réglages…" : "Autoriser…") {
                            Task {
                                await permission.request()
                                reload()
                            }
                        }
                        .buttonStyle(.notionPrimary)
                        .accessibilityLabel("Autoriser : \(permission.title)")
                        .accessibilityIdentifier(permission.identifier)
                    }
                }
            }
        }
        // S'arrête avec la fenêtre (la tâche est annulée à sa disparition).
        .task {
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

/// Ouvrir sillon à la connexion.
struct StartupSection: View {
    @State private var login = false

    var body: some View {
        SettingsGroup(title: "Démarrage") {
            SettingRow(title: "Ouvrir sillon à la connexion", detail: "Dans la barre des menus, prêt à lire") {
                Toggle("Ouvrir sillon à la connexion", isOn: $login)
                    .notionSwitch()
                    .onAppear { login = LoginItem.isEnabled }
                    .onChange(of: login) { _, on in
                        // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                        if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                    }
                    .accessibilityLabel("Ouvrir sillon à la connexion")
                    .accessibilityIdentifier("ouvrir-connexion")
            }
        }
    }
}

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

    var body: some View {
        SettingsGroup(title: "Voix") {
            ModelsTable(rows: rows.filter { $0.kind == "speech" }, model: model,
                        fallback: "La lecture passera à la voix de macOS.", onChange: onChange)
            if !store.capabilities.voices.isEmpty && !Self.suggestsModels(model.snapshot.voice) {
                SettingRow(title: "Voix", detail: "Lit tout le texte, quelle que soit sa langue") {
                    HStack(spacing: 8) {
                        PopUpPicker(label: "Voix", identifier: "voix-unique",
                                    options: store.capabilities.voices.map { (title: $0.label, value: $0.id) },
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
                    .padding(.vertical, 4)
                }
                if store.capabilities.family == "system" {
                    Link("Ajouter des voix dans Réglages Système…",
                         destination: URL(string: "x-apple.systempreferences:com.apple.preference.universalaccess")!)
                        .font(Typo.detail)
                        .padding(.bottom, 4)
                }
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
        // Un aperçu qui échoue après coup (synthèse refusée) se dit aussi.
        .onChange(of: model.snapshot.preview.error) { _, error in
            if let error { Announcement.post(error) }
        }
    }

    /// Voix de repli ou voix muette.
    static func suggestsModels(_ state: VoiceState) -> Bool {
        switch state {
        case .fallback, .unavailable: true
        default: false
        }
    }

    /// La voix en place : la française (une voix pour tout ; la voix de
    /// macOS garde la sienne dans `voice`) ; absente du modèle : sa première.
    static func voice(in values: SettingsView, _ c: ModelCapabilities) -> String {
        let chosen = c.family == "system" ? values.voice
            : values.voicesByLanguage.first(where: { $0.language == "fr" })?.voice
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
        SettingsGroup(title: "Raccourcis") {
            ForEach(Self.rows, id: \.identifier) { row in
                SettingRow(title: row.title, detail: row.detail) {
                    KeyboardShortcuts.Recorder(for: row.name)
                        .frame(width: 150)
                        .accessibilityLabel(row.title)
                        .accessibilityIdentifier(row.identifier)
                }
            }
            Callout(symbol: "info.circle", text: Self.voiceOverNote, identifier: "raccourcis-note-voiceover")
        }
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

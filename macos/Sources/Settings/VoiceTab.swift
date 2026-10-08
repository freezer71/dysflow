import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Onglet Voix : modèle actif, voix et « Écouter », langue, vitesse, volume,
/// voix clonée quand le modèle l'exige.
struct VoiceTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var installed: [VoiceModel] = []
    @State private var modelError = ErrorNote()
    /// Le focus va à la liste des voix : flèches pour choisir, Espace pour
    /// écouter. Sinon il tombe sur le premier champ de texte, et Espace y
    /// tape un espace.
    @FocusState private var voiceListFocused: Bool

    var body: some View {
        Form {
            ErrorText(store.error, identifier: "erreur-reglages")
            Section("Modèle") {
                LabeledContent("Modèle actif") {
                    PopUpPicker(label: "Modèle actif", identifier: "modele-actif",
                                options: Self.modelOptions(installed), selection: activeModel)
                        .fixedSize()
                }
                VoiceStateLine(state: model.snapshot.voice)
                if case .fallback = model.snapshot.voice {
                    Button("Voir les modèles") { SettingsNavigation.shared.select(.models) }
                }
                ErrorText(modelError.text, identifier: "erreur-modele-actif")
            }
            Section("Voix") {
                voicePicker
                HStack {
                    Button("Écouter") { Task { await store.listen() } }
                        .accessibilityIdentifier("ecouter")
                    Button("Arrêter") { Task { await store.stopListening() } }
                        .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                    if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                        Text(line).accessibilityIdentifier("etat-ecoute")
                    }
                }
                if store.isShown(.language) {
                    IgnorableNote(ignored: store.isIgnored(.language)) {
                        LabeledContent("Langue") {
                            PopUpPicker(label: "Langue", identifier: "langue",
                                        options: Self.languageOptions(store.capabilities.languages,
                                                                      current: store.values.language),
                                        selection: store.binding(\.language))
                                .fixedSize()
                        }
                    }
                }
                if store.isShown(.speed) {
                    IgnorableNote(ignored: store.isIgnored(.speed)) {
                        Slider(value: rounded(\.speed), in: 0.5...3, step: 0.05) {
                            Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                        } onEditingChanged: { store.dragChanged($0) }
                        .accessibilityLabel("Vitesse de lecture")
                        .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
                    }
                }
                // Le volume s'applique tout de suite, sans resynthèse.
                Slider(value: rounded(\.volume), in: 0...2, step: 0.05) {
                    Text("Volume : \(SettingFormat.percent(store.values.volume))")
                }
                .accessibilityLabel("Volume")
                .accessibilityValue(SettingFormat.percent(store.values.volume))
            }
            if [SettingKey.pitch, .energy, .emotion, .instruction].contains(where: { store.isShown($0) }) {
                Section("Expression") { expression }
            }
            Toggle("Afficher aussi les réglages ignorés par ce modèle", isOn: $store.showIgnored)
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-voix")
        .task { installed = await model.loadVoices().filter(\.installed) }
        // Un aperçu qui échoue après coup (synthèse refusée) se dit aussi.
        .onChange(of: model.snapshot.preview.error) { _, error in
            if let error { Announcement.post(error) }
        }
    }

    @ViewBuilder private var voicePicker: some View {
        let voices = store.capabilities.voices
        if !voices.isEmpty {
            List(voices, id: \.id, selection: voiceSelection) { voice in
                Text(voice.label).tag(voice.id)
            }
            .frame(height: 150)
            .accessibilityLabel("Voix du modèle")
            .accessibilityIdentifier("liste-voix")
            .focused($voiceListFocused)
            .onAppear { DispatchQueue.main.async { voiceListFocused = true } }
            // Espace fait entendre la voix choisie, comme « Écouter ». À
            // l'appui seulement : Espace tenu ne relance pas la synthèse.
            .onKeyPress(.space, phases: .down) { _ in
                Task { await store.listen() }
                return .handled
            }
            if let unlisted = Self.unlistedVoice(store.values.voice, voices) {
                Text("Voix en place : \(unlisted) (hors liste)")
            }
        }
        if Self.needsReference(store.capabilities) {
            LabeledContent("Voix clonée") {
                HStack {
                    Text(store.values.voice.isEmpty
                         ? "Aucun fichier"
                         : URL(fileURLWithPath: store.values.voice).lastPathComponent)
                    Button("Choisir un fichier WAV…") {
                        if let path = Self.chooseWav() { store.set(\.voice, path) }
                    }
                }
            }
        }
    }

    @ViewBuilder private var expression: some View {
        // Hauteur et énergie relancent la synthèse : écrites au relâchement.
        if store.isShown(.pitch) {
            IgnorableNote(ignored: store.isIgnored(.pitch)) {
                Slider(value: rounded(\.pitch), in: -1...1, step: 0.05) {
                    Text("Hauteur : \(SettingFormat.decimal(store.values.pitch))")
                } onEditingChanged: { store.dragChanged($0) }
                .accessibilityLabel("Hauteur")
                .accessibilityValue(SettingFormat.decimal(store.values.pitch))
            }
        }
        if store.isShown(.energy) {
            IgnorableNote(ignored: store.isIgnored(.energy)) {
                Slider(value: rounded(\.energy), in: 0...2, step: 0.05) {
                    Text("Énergie : \(SettingFormat.decimal(store.values.energy))")
                } onEditingChanged: { store.dragChanged($0) }
                .accessibilityLabel("Énergie")
                .accessibilityValue(SettingFormat.decimal(store.values.energy))
            }
        }
        if store.isShown(.emotion) {
            IgnorableNote(ignored: store.isIgnored(.emotion)) {
                LabeledContent("Intonation") {
                    PopUpPicker(label: "Intonation", identifier: "intonation",
                                options: Choices.including(Self.emotions, current: store.values.emotion) { $0 },
                                selection: store.binding(\.emotion))
                        .fixedSize()
                }
            }
        }
        if store.isShown(.instruction) {
            IgnorableNote(ignored: store.isIgnored(.instruction)) {
                TextField("Consigne de style", text: store.binding(\.instruction))
                    .accessibilityLabel("Consigne de style")
            }
        }
    }

    private var voiceSelection: Binding<String?> {
        Binding(get: { store.values.voice.isEmpty ? store.capabilities.voices.first?.id : store.values.voice },
                set: { id in
                    guard let id, id != store.values.voice else { return }
                    store.set(\.voice, id)
                })
    }

    private var activeModel: Binding<String> {
        Binding(get: { installed.first(where: \.active)?.id ?? "" },
                set: { id in
                    guard !id.isEmpty else { return }
                    Task {
                        do {
                            try await model.useVoiceAsync(id)
                            modelError.clear()
                        } catch {
                            modelError.fail(SettingsStore.message(of: error))
                        }
                        installed = await model.loadVoices().filter(\.installed)
                        // Le modèle change : voix, langues et réglages honorés
                        // aussi. Ce qui attendait sa pause part d'abord.
                        await store.reload()
                    }
                })
    }

    static let emotions: [(title: String, value: String)] = [
        ("Aucune", ""), ("Neutre", "neutre"), ("Calme", "calme"), ("Chaleureux", "chaleureux"),
        ("Sérieux", "sérieux"), ("Enjoué", "enjoué"), ("Posé", "posé"),
    ]

    /// Les modèles installés ; sans modèle actif, la voix du système d'abord.
    static func modelOptions(_ installed: [VoiceModel]) -> [(title: String, value: String)] {
        let models = installed.map { (title: $0.name, value: $0.id) }
        return installed.contains(where: \.active) ? models : [(title: "Voix du système", value: "")] + models
    }

    static func languageOptions(_ codes: [String], current: String) -> [(title: String, value: String)] {
        Choices.including(codes.map { (title: LanguageName.display($0), value: $0) }, current: current) {
            "\(LanguageName.display($0)) (hors liste)"
        }
    }

    /// L'état de la voix, une seule ligne : en chargement, ou la voix du
    /// système en repli avec la raison. Les voix listées sont alors celles du
    /// modèle configuré, pas celle qu'on entend.
    static func voiceLine(_ state: VoiceState) -> String? {
        switch state {
        case .loading: "Voix en chargement…"
        case .fallback(let reason): "Voix du système en repli : \(reason)"
        default: nil
        }
    }

    /// Un curseur à deux décimales : pas de 1,0500001 dans config.toml.
    private func rounded(_ key: WritableKeyPath<SettingsView, Float>) -> Binding<Float> {
        Binding(get: { store.values[keyPath: key] }, set: { store.set(key, SettingFormat.rounded($0)) })
    }

    static func previewLine(_ preview: PreviewView, refusal: String?) -> String? {
        if let refusal { return refusal }
        if let error = preview.error { return error }
        if preview.pending { return "Synthèse de l'aperçu…" }
        if preview.playing { return "Écoute…" }
        return nil
    }

    static func unlistedVoice(_ voice: String, _ voices: [VoiceChoice]) -> String? {
        voice.isEmpty || voices.contains(where: { $0.id == voice }) ? nil : voice
    }

    /// Un modèle qui clone, ou qui n'a aucune voix intégrée.
    static func needsReference(_ c: ModelCapabilities) -> Bool {
        c.requiresReference || (c.voices.isEmpty && !c.family.isEmpty)
    }

    @MainActor
    static func chooseWav() -> String? {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.wav]
        panel.allowsMultipleSelection = false
        panel.prompt = "Choisir"
        return panel.runModal() == .OK ? panel.url?.path : nil
    }
}

/// L'état de la voix (chargement, repli), dans les onglets qui la rechargent.
struct VoiceStateLine: View {
    let state: VoiceState

    var body: some View {
        if let line = VoiceTab.voiceLine(state) {
            Text(line).accessibilityIdentifier("etat-voix")
        }
    }
}

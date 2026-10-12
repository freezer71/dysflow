import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Onglet Voix : modèle actif, voix et « Écouter », langue, vitesse, volume,
/// voix clonée quand le modèle l'exige. Kokoro : une voix et un « Écouter »
/// par langue, la langue du document ouvert, l'avis et l'offre du passage.
struct VoiceTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var installed: [VoiceModel] = []
    @State private var modelError = ErrorNote()
    /// Le paquet accepté depuis l'offre : sa progression s'affiche ici.
    @State private var acceptedOffer: String?
    @State private var answeringOffer = false
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
                VoiceStateLine(state: model.snapshot.voice, preparing: model.voicePreparation.isRunning)
                if case .fallback = model.snapshot.voice {
                    Button("Voir les modèles") { SettingsNavigation.shared.select(.models) }
                }
                if let notice = model.snapshot.notice {
                    HStack {
                        Text(notice).accessibilityIdentifier("avis-voix")
                        Spacer()
                        Button("Compris") { model.dismissNotice() }
                            .accessibilityLabel("Compris, fermer l'avis de la nouvelle voix")
                    }
                }
                offer
                ErrorText(modelError.text, identifier: "erreur-modele-actif")
            }
            Section("Voix") {
                if Self.followsLanguage(store.capabilities) {
                    languageVoicePicker("fr", title: "Voix française", identifier: "voix-francaise")
                    languageVoicePicker("en", title: "Voix anglaise", identifier: "voix-anglaise")
                } else {
                    voicePicker
                }
                HStack {
                    if !Self.followsLanguage(store.capabilities) {
                        Button("Écouter") { Task { await store.listen() } }
                            .accessibilityIdentifier("ecouter")
                    }
                    Button("Arrêter") { Task { await store.stopListening() } }
                        .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                        .accessibilityLabel("Arrêter l'écoute")
                    if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                        Text(line).accessibilityIdentifier("etat-ecoute")
                    }
                }
                documentLanguage
                if store.isShown(.language) {
                    IgnorableNote(ignored: store.isIgnored(.language)) {
                        let title = Self.followsLanguage(store.capabilities) ? "Langue par défaut" : "Langue"
                        LabeledContent(title) {
                            PopUpPicker(label: title, identifier: "langue",
                                        options: Self.languageOptions(store.capabilities.languages,
                                                                      current: store.values.language),
                                        selection: store.binding(\.language))
                                .fixedSize()
                        }
                    }
                }
                // Vitesse et volume s'appliquent au son, tout de suite et avec
                // toutes les voix : ni resynthèse, ni réglage ignoré.
                Slider(value: rounded(\.speed), in: 0.5...3, step: 0.05) {
                    Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                }
                .accessibilityLabel("Vitesse de lecture")
                .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
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
        // L'avis et l'offre du passage à Kokoro se disent à VoiceOver.
        .onChange(of: model.snapshot.notice, initial: true) { _, notice in
            if let notice { Announcement.post(notice) }
        }
        .onChange(of: model.snapshot.offer, initial: true) { _, offer in
            if let offer { Announcement.post("Nouvelle voix disponible. \(Self.offerTitle(offer)) ?") }
        }
        .onChange(of: model.voicePreparation.isRunning) { _, running in
            if running { Announcement.post("Voix en préparation, la première fois") }
        }
    }

    /// M1 : l'offre en un clic, taille comprise ; « Plus tard » la retient
    /// (Kokoro reste installable depuis l'onglet Modèles). Après l'accord,
    /// le téléchargement avance ici.
    @ViewBuilder private var offer: some View {
        if let offer = model.snapshot.offer {
            VStack(alignment: .leading, spacing: 6) {
                Text("Nouvelle voix disponible : Kokoro, plus naturelle, en français et en anglais.")
                    .accessibilityIdentifier("offre-voix")
                HStack {
                    Button(Self.offerTitle(offer)) { accept(offer) }
                        .accessibilityIdentifier("passer-nouvelle-voix")
                    Button("Plus tard") {
                        answeringOffer = true
                        Task {
                            await model.dismissVoiceOffer()
                            answeringOffer = false
                        }
                    }
                    .accessibilityLabel("Plus tard : garder la voix actuelle")
                    .accessibilityIdentifier("plus-tard")
                    if answeringOffer { ProgressView().controlSize(.small).accessibilityLabel("Réponse en cours") }
                }
                .disabled(answeringOffer)
            }
        } else if let id = acceptedOffer, let d = model.snapshot.download, d.id == id, d.failed == nil {
            LabeledContent("Téléchargement de la nouvelle voix") {
                ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                    .frame(width: 160)
                    .accessibilityLabel("Téléchargement de la nouvelle voix")
                    .accessibilityValue(SettingFormat.percent(Float(d.received) / Float(max(d.total, 1))))
            }
        }
    }

    private func accept(_ offer: VoiceOfferView) {
        answeringOffer = true
        Task {
            do {
                try await model.acceptVoiceOffer()
                acceptedOffer = offer.id
                modelError.clear()
            } catch {
                modelError.fail(SettingsStore.message(of: error))
            }
            answeringOffer = false
            // Kokoro déjà installé : il est actif tout de suite.
            installed = await model.loadVoices().filter(\.installed)
            await store.reload()
        }
    }

    /// Une voix par langue (Kokoro), chacune avec son « Écouter ».
    @ViewBuilder private func languageVoicePicker(_ language: String, title: String, identifier: String) -> some View {
        LabeledContent(title) {
            HStack {
                PopUpPicker(label: title, identifier: identifier,
                            options: Self.voiceOptions(store.capabilities, language: language, values: store.values),
                            selection: Binding(get: { Self.voice(for: language, in: store.values, store.capabilities) },
                                               set: { store.setVoice($0, language: language) }))
                    .fixedSize()
                Button("Écouter") { Task { await store.listen(language: language) } }
                    .accessibilityLabel("Écouter la \(title.lowercased())")
                    .accessibilityIdentifier("ecouter-\(language)")
            }
        }
    }

    /// La langue du document ouvert, et la mention d'un document mêlé.
    @ViewBuilder private var documentLanguage: some View {
        if let language = model.snapshot.documentLanguage {
            LabeledContent("Langue de ce document") {
                PopUpPicker(label: "Langue de ce document", identifier: "langue-document",
                            options: Self.documentLanguageOptions(language),
                            selection: Binding(get: { language.chosen ?? "" }, set: { value in
                                Task {
                                    do {
                                        try await model.setDocumentLanguage(value.isEmpty ? nil : value)
                                        modelError.clear()
                                    } catch {
                                        modelError.fail(SettingsStore.message(of: error))
                                    }
                                }
                            }))
                    .fixedSize()
            }
            if let line = Self.mixedLine(language) {
                Text(line).accessibilityIdentifier("document-mele")
            }
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

    /// « Automatique » d'abord, puis les langues du modèle et celle en place.
    static func languageOptions(_ codes: [String], current: String) -> [(title: String, value: String)] {
        let options = [(title: "Automatique", value: "auto")] + codes.map { (title: LanguageName.display($0), value: $0) }
        return Choices.including(options, current: current) { "\(LanguageName.display($0)) (hors liste)" }
    }

    /// Kokoro choisit sa voix par langue.
    static func followsLanguage(_ c: ModelCapabilities) -> Bool {
        c.family == "kokoro"
    }

    static func voices(_ c: ModelCapabilities, language: String) -> [VoiceChoice] {
        c.voices.filter { $0.language == language }
    }

    /// La voix en place pour une langue ; rien de choisi : la première.
    static func voice(for language: String, in values: SettingsView, _ c: ModelCapabilities) -> String {
        values.voicesByLanguage.first(where: { $0.language == language })?.voice
            ?? voices(c, language: language).first?.id ?? ""
    }

    /// Les voix d'une langue, plus celle en place si elle n'y est pas
    /// (écrite à la main dans `config.toml`).
    static func voiceOptions(_ c: ModelCapabilities, language: String,
                             values: SettingsView) -> [(title: String, value: String)] {
        let options = voices(c, language: language).map { (title: $0.label, value: $0.id) }
        let current = voice(for: language, in: values, c)
        return current.isEmpty ? options : Choices.including(options, current: current) { "\($0) (hors liste)" }
    }

    static func documentLanguageOptions(_ d: DocumentLanguageView) -> [(title: String, value: String)] {
        let detected = d.detected.map { LanguageName.display($0).lowercased() } ?? "non détectée"
        return [(title: "Automatique (\(detected))", value: ""), (title: "Français", value: "fr"),
                (title: "Anglais", value: "en")]
    }

    /// « Document mêlé : 62 % en français » : la part de la langue
    /// majoritaire, au-delà de 30 % dans l'autre.
    static func mixedLine(_ d: DocumentLanguageView) -> String? {
        guard d.mixed else { return nil }
        let majority = Int(((1 - d.minorityShare) * 100).rounded())
        return "Document mêlé : \(majority) % en \(LanguageName.display(d.detected ?? d.effective).lowercased())"
    }

    /// Le bouton de l'offre, avec la taille écrite comme dans l'onglet
    /// Modèles (`ByteCountFormatter`, base 1000) : « Passer à la nouvelle
    /// voix (342 Mo) ».
    static func offerTitle(_ offer: VoiceOfferView) -> String {
        "Passer à la nouvelle voix (\(ByteCountFormatter.string(fromByteCount: Int64(offer.bytes), countStyle: .file)))"
    }

    /// L'état de la voix, une seule ligne : la voix du système en repli avec
    /// la raison (les voix listées sont alors celles du modèle configuré, pas
    /// celle qu'on entend), sinon la préparation des noyaux Metal (la
    /// première fois, à part du lecteur), sinon le chargement.
    static func voiceLine(_ state: VoiceState, preparing: Bool = false) -> String? {
        if case .fallback(let reason) = state { return "Voix du système en repli : \(reason)" }
        if preparing { return "Voix en préparation (première fois)…" }
        if case .loading = state { return "Voix en chargement…" }
        return nil
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
    var preparing = false

    var body: some View {
        if let line = VoiceTab.voiceLine(state, preparing: preparing) {
            Text(line).accessibilityIdentifier("etat-voix")
        }
    }
}

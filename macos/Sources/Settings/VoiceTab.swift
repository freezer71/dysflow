import SwiftUI

/// Onglet Voix, réduit à l'essentiel (14 oct. 2026). Quand Kokoro parle
/// (ou se charge, ou se prépare) : une voix et un « Écouter » par langue,
/// la langue du document ouvert, la vitesse, le volume. Quand il ne parle
/// pas — pas encore installé, ou configuré mais en échec, la voix du
/// système lisant en repli — : l'état, l'offre ou « Voir les modèles », la
/// vitesse et le volume, rien d'autre (`showsKokoroVoices`). Au-dessus,
/// seulement quand il y a lieu : l'état de la voix, l'avis et l'offre.
struct VoiceTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var modelError = ErrorNote()
    /// Le paquet accepté depuis l'offre : sa progression s'affiche ici.
    @State private var acceptedOffer: String?
    @State private var answeringOffer = false

    var body: some View {
        Form {
            ErrorText(store.error, identifier: "erreur-reglages")
            VoiceStateLine(state: model.snapshot.voice, preparing: model.voicePreparation.isRunning)
            if Self.suggestsModels(model.snapshot.voice) {
                Button("Voir les modèles") { SettingsNavigation.shared.select(.models) }
            }
            if let notice = model.snapshot.notice {
                HStack {
                    Text(notice).accessibilityIdentifier("avis-voix")
                    Spacer()
                    Button("Compris") { Task { await model.dismissNotice() } }
                        .accessibilityLabel("Compris, fermer l'avis de la nouvelle voix")
                }
            }
            offer
            ErrorText(modelError.text, identifier: "erreur-modele-actif")
            Section {
                if Self.showsKokoroVoices(store.capabilities, voice: model.snapshot.voice) {
                    languageVoicePicker("fr", title: "Voix française", identifier: "voix-francaise")
                    languageVoicePicker("en", title: "Voix anglaise", identifier: "voix-anglaise")
                    if let line = Self.previewLine(model.snapshot.preview, refusal: store.previewNote) {
                        HStack {
                            Text(line).accessibilityIdentifier("etat-ecoute")
                            Spacer()
                            Button("Arrêter") { Task { await store.stopListening() } }
                                .disabled(!(model.snapshot.preview.pending || model.snapshot.preview.playing))
                                .accessibilityLabel("Arrêter l'écoute")
                        }
                    }
                    documentLanguage
                }
                // Vitesse et volume s'appliquent au son, tout de suite et avec
                // toutes les voix, celle du système comprise.
                Slider(value: rounded(\.speed), in: 0.5...3, step: 0.05) {
                    Text("Vitesse : \(SettingFormat.decimal(store.values.speed))×")
                }
                .accessibilityLabel("Vitesse de lecture")
                .accessibilityIdentifier("vitesse")
                .accessibilityValue("\(SettingFormat.decimal(store.values.speed)) fois")
                Slider(value: rounded(\.volume), in: 0...2, step: 0.05) {
                    Text("Volume : \(SettingFormat.percent(store.values.volume))")
                }
                .accessibilityLabel("Volume")
                .accessibilityValue(SettingFormat.percent(store.values.volume))
            }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-voix")
        // Un aperçu qui échoue après coup (synthèse refusée) se dit aussi.
        .onChange(of: model.snapshot.preview.error) { _, error in
            if let error { Announcement.post(error) }
        }
        // L'avis et l'offre du passage à Kokoro se disent à VoiceOver.
        // Une fois par valeur : pas à chaque retour sur l'onglet.
        .onChange(of: model.snapshot.notice, initial: true) { _, notice in
            if let notice { AnnounceOnce.post(notice) }
        }
        .onChange(of: model.snapshot.offer, initial: true) { _, offer in
            if let offer { AnnounceOnce.post("Nouvelle voix disponible. \(Self.offerTitle(offer)) ?") }
        }
        // L'échec d'un téléchargement accepté se dit déjà par ReglagesView
        // (« Téléchargement interrompu : … », quel que soit l'onglet) : pas
        // de seconde annonce ici.
        .onChange(of: model.voicePreparation.isRunning) { _, running in
            if running { Announcement.post("Voix en préparation, la première fois") }
        }
    }

    /// M1 : l'offre en un clic, taille comprise ; « Plus tard » la retient
    /// (Kokoro reste installable depuis l'onglet Modèles). Après l'accord,
    /// le téléchargement avance ici.
    @ViewBuilder private var offer: some View {
        if let failure = Self.acceptedDownloadFailure(accepted: acceptedOffer, download: model.snapshot.download) {
            ErrorText(failure, identifier: "erreur-nouvelle-voix")
        }
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
            // Kokoro déjà installé : il est actif tout de suite, ses voix aussi.
            await store.reload()
        }
    }

    /// Une voix par langue (Kokoro), chacune avec son « Écouter ».
    @ViewBuilder private func languageVoicePicker(_ language: String, title: String, identifier: String) -> some View {
        LabeledContent(title) {
            HStack {
                PopUpPicker(label: title, identifier: identifier,
                            options: Self.voiceOptions(store.capabilities, language: language),
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

    /// Kokoro choisit sa voix par langue.
    static func followsLanguage(_ c: ModelCapabilities) -> Bool {
        c.family == "kokoro"
    }

    /// Les voix Kokoro ne se montrent que si Kokoro parle vraiment : il est
    /// la famille configurée, et la voix n'est ni en repli sur celle du
    /// système ni indisponible. Sinon « Écouter la voix française » ferait
    /// entendre `say` sous le nom de p-fenrir60. Chargement et préparation
    /// gardent les voix : c'est Kokoro qui arrive.
    static func showsKokoroVoices(_ c: ModelCapabilities, voice: VoiceState) -> Bool {
        followsLanguage(c) && !suggestsModels(voice)
    }

    static func voices(_ c: ModelCapabilities, language: String) -> [VoiceChoice] {
        c.voices.filter { $0.language == language }
    }

    /// La voix en place pour une langue. Rien de choisi, ou une voix qui
    /// n'est plus proposée (retirée du catalogue, écrite à la main) : la
    /// première de la langue, la recommandée — celle que le moteur fait
    /// parler (`models::voice_for_language`).
    static func voice(for language: String, in values: SettingsView, _ c: ModelCapabilities) -> String {
        let listed = voices(c, language: language)
        let chosen = values.voicesByLanguage.first(where: { $0.language == language })?.voice
        return listed.first(where: { $0.id == chosen })?.id ?? listed.first?.id ?? ""
    }

    /// Les voix d'une langue, rien d'autre.
    static func voiceOptions(_ c: ModelCapabilities, language: String) -> [(title: String, value: String)] {
        voices(c, language: language).map { (title: $0.label, value: $0.id) }
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

    /// Le bouton de l'offre, avec la taille écrite comme partout dans l'app
    /// (`SizeFormat`) : « Passer à la nouvelle voix (342 Mo) ».
    static func offerTitle(_ offer: VoiceOfferView) -> String {
        "Passer à la nouvelle voix (\(SizeFormat.bytes(offer.bytes)))"
    }

    /// L'avis du lancement quand une offre attend : où y répondre. La
    /// phrase du moteur (taille en base 1024, question sans bouton) n'est
    /// pas montrée.
    static func launchNotice(_ offer: VoiceOfferView) -> String {
        "Nouvelle voix disponible (\(SizeFormat.bytes(offer.bytes))) : Réglages › Voix"
    }

    /// Le téléchargement accepté depuis cet onglet a échoué : l'offre est
    /// revenue, l'échec se dit au-dessus.
    static func acceptedDownloadFailure(accepted: String?, download: DownloadView?) -> String? {
        guard let accepted, let d = download, d.id == accepted, let failed = d.failed else { return nil }
        return "Téléchargement interrompu : \(failed)"
    }

    /// Voix de repli ou voix muette (aucune session ne s'ouvre) : un autre
    /// modèle est la piste.
    static func suggestsModels(_ state: VoiceState) -> Bool {
        switch state {
        case .fallback, .unavailable: true
        default: false
        }
    }

    /// L'état de la voix, une seule ligne : la voix du système en repli avec
    /// la raison, sinon la préparation des noyaux Metal (la
    /// première fois, à part du lecteur), sinon le chargement.
    static func voiceLine(_ state: VoiceState, preparing: Bool = false) -> String? {
        if case .fallback(let reason) = state { return "Voix du système en repli : \(reason)" }
        if case .unavailable(let reason) = state { return "Voix indisponible : \(reason)" }
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

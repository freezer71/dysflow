import KeyboardShortcuts
import SwiftUI

/// Le nom `SettingsView` est pris par l'enregistrement du moteur (UniFFI).
struct ReglagesView: View {
    let model: ReaderModel

    var body: some View {
        TabView {
            ReadingSettings(model: model).tabItem { Label("Lecture", systemImage: "text.alignleft") }
            ShortcutSettings().tabItem { Label("Raccourcis", systemImage: "keyboard") }
            VoiceSettings(model: model).tabItem { Label("Voix", systemImage: "waveform") }
            GeneralSettings().tabItem { Label("Général", systemImage: "gearshape") }
        }
        .navigationTitle("Réglages")
        .frame(width: 520)
        .font(.system(size: 13))
    }
}

/// Le message d'une erreur du moteur, sans l'habillage de l'enum.
private func message(of error: Error) -> String {
    if case SillonError.Failed(let message) = error { return message }
    return error.localizedDescription
}

private struct ReadingSettings: View {
    let model: ReaderModel
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize
    @State private var settings = ReadingValues()
    @State private var loaded = false
    @State private var draggingSpeed = false
    @State private var error: String?
    @State private var committer: Committer<ReadingValues>?

    var body: some View {
        Form {
            LabeledContent("Police de lecture") {
                PopUpPicker(label: "Police de lecture", identifier: "police-lecture",
                            options: ReadingFont.allCases.map { ($0.label, $0.rawValue) },
                            selection: $fontName)
                    .fixedSize()
            }
            Slider(value: $size, in: 14...40, step: 1) { Text("Taille : \(Int(size)) pt") }
                .accessibilityLabel("Taille du texte")
                .accessibilityValue("\(Int(size)) points")
            Stepper("Interlignage : \(settings.lineSpacing)", value: $settings.lineSpacing, in: 0...3)
                .accessibilityLabel("Interlignage")
                .accessibilityValue("\(settings.lineSpacing)")
            Toggle("Règle de lecture", isOn: $settings.ruler)
            // La vitesse relance la synthèse : écrite au relâchement du curseur.
            Slider(value: $settings.speed, in: 0.5...3, step: 0.1) {
                Text(String(format: "Vitesse : %.1f×", settings.speed))
            } onEditingChanged: { editing in
                draggingSpeed = editing
                if !editing { committer?.change(settings, immediate: true) }
            }
            .accessibilityLabel("Vitesse de lecture")
            .accessibilityValue(String(format: "%.1f fois", settings.speed))
            if let error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .task {
            // Une seule fois : revenir sur l'onglet ne relit rien.
            guard !loaded else { return }
            let values = ReadingValues(await model.loadSettings())
            // Seul le réglage touché s'écrit, par-dessus les réglages relus
            // juste avant : la vitesse changée au clavier entre-temps reste.
            let c = Committer<ReadingValues>(saved: values, writeChange: { [model] new, old in
                _ = try await model.updateSettings(ReadingValues.patch(new, over: old))
            }, onResult: { error = $0.map(message(of:)) })
            committer = c
            settings = values
            loaded = true
        }
        .onChange(of: settings) { _, new in
            if !draggingSpeed { committer?.change(new) }
        }
    }
}

/// Les réglages de lecture, en types que SwiftUI lie directement.
struct ReadingValues: Equatable {
    var lineSpacing = 1
    var ruler = true
    var speed = 1.0

    init() {}
    init(_ s: SettingsView) {
        lineSpacing = Int(s.lineSpacing)
        ruler = s.readingRuler
        speed = Double(s.speed)
    }

    /// Le patch des seuls champs que `new` change par rapport à `old`.
    static func patch(_ new: ReadingValues, over old: ReadingValues) -> SettingsPatch {
        var p = SettingsPatch.empty
        if new.lineSpacing != old.lineSpacing { p.lineSpacing = UInt32(new.lineSpacing) }
        if new.ruler != old.ruler { p.readingRuler = new.ruler }
        if new.speed != old.speed { p.speed = Float(new.speed) }
        return p
    }
}

private struct ShortcutSettings: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
        }
        .formStyle(.grouped)
    }
}

private struct VoiceSettings: View {
    let model: ReaderModel
    @State private var voices: [VoiceModel] = []
    @State private var keep: KeepVoiceChoice = .minutes(10)
    /// La valeur stockée par le moteur, nil tant qu'elle n'est pas chargée.
    @State private var storedKeep: Int64?
    @State private var error: String?

    /// Vrai tant qu'un téléchargement est en cours (pas un échec resté là).
    private var downloading: Bool {
        guard let d = model.snapshot.download else { return false }
        return d.failed == nil
    }

    var body: some View {
        Form {
            Section("Voix") {
                ForEach(voices, id: \.id) { voice in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(voice.name)
                            // Couleur du texte, non « secondaire » : le gris
                            // système, sur le fond clair d'un formulaire,
                            // n'atteint pas 4,5:1.
                            Text(ByteCountFormatter.string(fromByteCount: Int64(voice.bytes), countStyle: .file))
                            if let d = model.snapshot.download, d.id == voice.id, let failed = d.failed {
                                Text("Échec du téléchargement : \(failed)").foregroundStyle(.red)
                            }
                        }
                        Spacer()
                        action(for: voice)
                    }
                    .accessibilityElement(children: .contain)
                }
            }
            LabeledContent("Garder la voix chargée") {
                PopUpPicker(label: "Garder la voix chargée", identifier: "garder-voix",
                            options: KeepVoiceChoice.allCases.map { ($0.label, $0) },
                            selection: $keep)
                    .fixedSize()
            }
            if let error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .task {
            voices = await model.loadVoices()
            let stored = await model.loadSettings().keepVoiceMinutes
            keep = KeepVoiceChoice(minutes: stored)
            storedKeep = stored
        }
        .onChange(of: keep) { _, new in
            guard KeepVoiceChoice.needsSave(choice: new, stored: storedKeep) else { return }
            Task {
                await act {
                    var p = SettingsPatch.empty
                    p.keepVoiceMinutes = new.minutes
                    _ = try await model.updateSettings(p)
                }
                if error == nil { storedKeep = new.minutes }
            }
        }
        // À l'ouverture : l'état courant (un téléchargement peut avoir avancé).
        .onAppear { model.refresh(); refresh() }
        // Un téléchargement qui finit ou échoue change l'état des voix.
        .onChange(of: model.snapshot.download?.failed) { _, _ in refresh() }
        .onChange(of: downloading) { _, _ in refresh() }
    }

    @ViewBuilder
    private func action(for voice: VoiceModel) -> some View {
        if voice.active {
            Text("Active")
        } else if voice.installed {
            Button("Utiliser") { run { try await model.useVoiceAsync(voice.id) } }
                .accessibilityLabel("Utiliser la voix \(voice.name)")
        } else if let d = model.snapshot.download, d.id == voice.id, d.failed == nil {
            ProgressView(value: Double(d.received), total: Double(max(d.total, 1)))
                .frame(width: 120)
                .accessibilityLabel("Téléchargement de \(voice.name)")
        } else {
            // Après un échec, le même bouton relance l'installation.
            Button(model.snapshot.download?.id == voice.id ? "Réessayer" : "Installer") {
                run { try await model.installVoiceAsync(voice.id) }
            }
            .accessibilityLabel("Installer la voix \(voice.name)")
            .disabled(downloading)
        }
    }

    private func run(_ f: @escaping () async throws -> Void) {
        Task { await act(f) }
    }

    private func act(_ f: () async throws -> Void) async {
        do { try await f(); error = nil } catch { self.error = message(of: error) }
        voices = await model.loadVoices()
    }

    private func refresh() {
        Task { voices = await model.loadVoices() }
    }
}

private struct GeneralSettings: View {
    @State private var login = false

    var body: some View {
        Form {
            Toggle("Ouvrir sillon à la connexion", isOn: $login)
                .onAppear { login = LoginItem.isEnabled }
                .onChange(of: login) { _, on in
                    // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                    if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                }
                .accessibilityLabel("Ouvrir sillon à la connexion")
        }
        .formStyle(.grouped)
    }
}

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
    /// Ce que le moteur a déjà : un changement identique n'est pas renvoyé.
    @State private var saved: ReadingValues?
    @State private var pending: Task<Void, Never>?
    @State private var error: String?

    var body: some View {
        Form {
            Picker("Police", selection: $fontName) {
                ForEach(ReadingFont.allCases) { Text($0.label).tag($0.rawValue) }
            }
            .accessibilityLabel("Police de lecture")
            .accessibilityIdentifier("police-lecture")
            Slider(value: $size, in: 14...40, step: 1) { Text("Taille : \(Int(size)) pt") }
                .accessibilityLabel("Taille du texte")
                .accessibilityValue("\(Int(size)) points")
            Stepper("Interlignage : \(settings.lineSpacing)", value: $settings.lineSpacing, in: 0...4)
                .accessibilityLabel("Interlignage")
                .accessibilityValue("\(settings.lineSpacing)")
            Toggle("Règle de lecture", isOn: $settings.ruler)
            Slider(value: $settings.speed, in: 0.5...3, step: 0.1) {
                Text(String(format: "Vitesse : %.1f×", settings.speed))
            }
            .accessibilityLabel("Vitesse de lecture")
            .accessibilityValue(String(format: "%.1f fois", settings.speed))
            if let error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .task {
            let loaded = ReadingValues(await model.loadSettings())
            saved = loaded
            settings = loaded
        }
        .onChange(of: settings) { _, new in save(new) }
    }

    /// Une écriture à la fois, dans l'ordre, après une courte pause : un
    /// curseur qu'on glisse n'envoie pas une écriture par pas.
    private func save(_ new: ReadingValues) {
        guard let saved, new != saved else { return }
        let previous = pending
        pending = Task {
            await previous?.value
            try? await Task.sleep(for: .milliseconds(150))
            if Task.isCancelled { return }
            do {
                try await model.changeSettings {
                    $0.lineSpacing = UInt32(new.lineSpacing)
                    $0.readingRuler = new.ruler
                    $0.speed = Float(new.speed)
                }
                self.saved = new
                error = nil
            } catch {
                self.error = message(of: error)
            }
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
    @State private var keepLoaded = false
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
                            Text(ByteCountFormatter.string(fromByteCount: Int64(voice.bytes), countStyle: .file))
                                .foregroundStyle(.secondary)
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
            Picker("Garder la voix chargée", selection: $keep) {
                ForEach(KeepVoiceChoice.allCases, id: \.self) { Text($0.label).tag($0) }
            }
            .accessibilityLabel("Garder la voix chargée")
            .accessibilityIdentifier("garder-voix")
            if let error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .task {
            voices = await model.loadVoices()
            keep = KeepVoiceChoice(minutes: await model.loadSettings().keepVoiceMinutes)
            keepLoaded = true
        }
        .onChange(of: keep) { _, new in
            guard keepLoaded else { return }
            Task { await act { try await model.changeSettings { $0.keepVoiceMinutes = new.minutes } } }
        }
        // Un téléchargement qui finit ou échoue change l'état des voix.
        .onChange(of: model.snapshot.download?.failed) { _, _ in refresh() }
        .onChange(of: downloading) { _, _ in refresh() }
    }

    @ViewBuilder
    private func action(for voice: VoiceModel) -> some View {
        if voice.active {
            Text("Active").foregroundStyle(.secondary)
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
    @State private var login = LoginItem.isEnabled

    var body: some View {
        Form {
            Toggle("Ouvrir sillon à la connexion", isOn: $login)
                .onChange(of: login) { _, on in
                    // L'inscription peut être refusée : l'interrupteur dit l'état réel.
                    if LoginItem.isEnabled != on, !LoginItem.set(on) { login = LoginItem.isEnabled }
                }
                .accessibilityLabel("Ouvrir sillon à la connexion")
        }
        .formStyle(.grouped)
    }
}

import KeyboardShortcuts
import SwiftUI

/// Le message d'une erreur du moteur, sans l'habillage de l'enum.
private func message(of error: Error) -> String {
    if case SillonError.Failed(let message) = error { return message }
    return error.localizedDescription
}

struct ShortcutSettings: View {
    var body: some View {
        Form {
            KeyboardShortcuts.Recorder("Lire la sélection :", name: .readSelection)
            KeyboardShortcuts.Recorder("Afficher sillon :", name: .showPanel)
        }
        .formStyle(.grouped)
    }
}

struct VoiceSettings: View {
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

struct GeneralSettings: View {
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

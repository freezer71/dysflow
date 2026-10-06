import SwiftUI

/// Onglet Avancé : le moteur, l'échantillonnage, les options libres.
struct AdvancedTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore
    @State private var newKey = ""
    @State private var newValue = ""
    @State private var extraError = ErrorNote()

    var body: some View {
        Form {
            ErrorText(store.error, identifier: "erreur-reglages")
            Section("Moteur") {
                // Les changements d'ici rechargent la voix : on voit le chargement.
                VoiceStateLine(state: model.snapshot.voice)
                LabeledContent("Moteur") {
                    PopUpPicker(label: "Moteur", identifier: "moteur",
                                options: Self.engineOptions(current: store.values.engine),
                                selection: store.engineBinding(\.engine))
                        .fixedSize()
                }
                LabeledContent("Calcul") {
                    PopUpPicker(label: "Calcul", identifier: "calcul",
                                options: Self.computeOptions(current: store.values.compute),
                                selection: store.engineBinding(\.compute))
                        .fixedSize()
                }
                Stepper("Fils de calcul : \(store.values.threads)", value: store.engineBinding(\.threads), in: 1...64)
                    .accessibilityLabel("Fils de calcul")
                    .accessibilityValue("\(store.values.threads)")
                Stepper(Self.sessionsText(store.values.sessions), value: store.engineBinding(\.sessions), in: 0...16)
                    .accessibilityLabel("Sessions")
                    .accessibilityValue(Self.sessionsText(store.values.sessions))
                Stepper("Phrases préparées d'avance : \(store.values.prefetch)",
                        value: store.engineBinding(\.prefetch), in: 2...64)
                    .accessibilityLabel("Phrases préparées d'avance")
                    .accessibilityValue("\(store.values.prefetch)")
                if store.values.engine == "system" {
                    Text("Sans effet avec la voix du système")
                }
                if store.values.engine != "system" {
                    Text("Chaque changement ici recharge la voix ; la lecture reprend à la même phrase.")
                }
            }
            Section("Échantillonnage") {
                if store.values.engine == "system" {
                    Text("Sans effet avec la voix du système")
                }
                if store.isShown(.temperature) {
                    OptionalSlider(title: "Température", ignored: store.isIgnored(.temperature),
                                   value: store.binding(\.temperature), range: 0...2, step: 0.05, fallback: 0.7,
                                   onEditingChanged: { store.dragChanged($0) })
                }
                if store.isShown(.topP) {
                    OptionalSlider(title: "Top-p", ignored: store.isIgnored(.topP),
                                   value: store.binding(\.topP), range: 0...1, step: 0.05, fallback: 0.9,
                                   onEditingChanged: { store.dragChanged($0) })
                }
                if store.isShown(.topK) {
                    OptionalStepper(title: "Top-k", ignored: store.isIgnored(.topK),
                                    value: store.binding(\.topK), range: 1...200, fallback: 50)
                }
                if store.isShown(.guidance) {
                    OptionalSlider(title: "Guidage", ignored: store.isIgnored(.guidance),
                                   value: store.binding(\.guidanceScale), range: 0...20, step: 0.5, fallback: 3,
                                   onEditingChanged: { store.dragChanged($0) })
                }
                if store.isShown(.repetition) {
                    OptionalSlider(title: "Pénalité de répétition", ignored: store.isIgnored(.repetition),
                                   value: store.binding(\.repetitionPenalty), range: 0.5...2, step: 0.05,
                                   fallback: 1.1,
                                   onEditingChanged: { store.dragChanged($0) })
                }
                if store.isShown(.steps) {
                    OptionalStepper(title: "Pas d'inférence", ignored: store.isIgnored(.steps),
                                    value: store.binding(\.inferenceSteps), range: 1...200, fallback: 16)
                }
                if store.isShown(.seed) {
                    SeedField(ignored: store.isIgnored(.seed), seed: store.binding(\.seed))
                }
                Toggle("Afficher aussi les réglages ignorés par ce modèle", isOn: $store.showIgnored)
            }
            Section("Options du modèle") {
                ForEach(store.values.extra, id: \.key) { option in
                    HStack {
                        Text("\(option.key) = \(option.value)")
                        Spacer()
                        Button("Modifier") { newKey = option.key; newValue = option.value }
                            .accessibilityLabel("Modifier l'option \(option.key)")
                        Button("Retirer") {
                            store.set(\.extra, ExtraOptions.removing(option.key, from: store.values.extra))
                        }
                        .accessibilityLabel("Retirer l'option \(option.key)")
                    }
                }
                HStack {
                    TextField("Clé", text: $newKey).accessibilityLabel("Clé de l'option").onSubmit { add() }
                    TextField("Valeur", text: $newValue).accessibilityLabel("Valeur de l'option").onSubmit { add() }
                    Button("Ajouter") { add() }
                        .disabled(newKey.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                ErrorText(extraError.text, identifier: "erreur-option")
                Text("Transmises telles quelles au modèle (section [extra] de config.toml).")
            }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-avance")
        .onDisappear { store.endDragging() }
    }

    private func add() {
        guard let next = ExtraOptions.adding(key: newKey, value: newValue, to: store.values.extra) else {
            extraError.fail("Il faut une clé, sans « = ».")
            return
        }
        store.set(\.extra, next)
        newKey = ""
        newValue = ""
        extraError.clear()
    }

    static func engineOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Automatique", "auto"), ("audio.cpp", "audio_cpp"), ("Voix du système", "system")],
                          current: current) { "\($0) (écrit à la main)" }
    }

    static func computeOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Metal", "metal"), ("CPU", "cpu")], current: current) { "\($0) (écrit à la main)" }
    }

    static func sessionsText(_ n: UInt32) -> String {
        n == 0 ? "Sessions : automatique" : "Sessions : \(n)"
    }
}

/// Les options libres du modèle, en liste clé = valeur.
enum ExtraOptions {
    /// Ajoute ou remplace une option ; nil pour une clé vide ou qui contient « = ».
    static func adding(key: String, value: String, to options: [ExtraOption]) -> [ExtraOption]? {
        let key = key.trimmingCharacters(in: .whitespaces)
        guard !key.isEmpty, !key.contains("=") else { return nil }
        let value = value.trimmingCharacters(in: .whitespaces)
        let option = ExtraOption(key: key, value: value)
        guard let index = options.firstIndex(where: { $0.key == key }) else { return options + [option] }
        var next = options.filter { $0.key != key }
        next.insert(option, at: min(index, next.count))
        return next
    }

    static func removing(_ key: String, from options: [ExtraOption]) -> [ExtraOption] {
        options.filter { $0.key != key }
    }
}

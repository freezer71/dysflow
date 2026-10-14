import SwiftUI

/// Onglet Avancé : le moteur de synthèse.
struct AdvancedTab: View {
    let model: ReaderModel
    @Bindable var store: SettingsStore

    var body: some View {
        SettingsPage(title: "Avancé", subtitle: "Le moteur de synthèse. Les valeurs par défaut conviennent presque toujours.") {
            ErrorText(store.error, identifier: "erreur-reglages")
            // Les changements d'ici rechargent la voix : on voit le chargement.
            if let line = VoiceTab.voiceLine(model.snapshot.voice, preparing: model.voicePreparation.isRunning) {
                Callout(symbol: "waveform", text: line, identifier: "etat-voix")
            }
            SettingsGroup(title: "Moteur") {
                SettingRow(title: "Moteur", detail: store.values.engine == "system"
                           ? "Sans effet avec la voix du système"
                           : "Chaque changement ici recharge la voix ; la lecture reprend à la même phrase.") {
                    PopUpPicker(label: "Moteur", identifier: "moteur",
                                options: Self.engineOptions(current: store.values.engine),
                                selection: store.engineBinding(\.engine))
                        .fixedSize()
                }
                SettingRow(title: "Fils de calcul", detail: "Kokoro au terminal ; l'app calcule sur la puce graphique") {
                    ValueText("\(store.values.threads)", width: 30)
                    Stepper("Fils de calcul : \(store.values.threads)", value: store.engineBinding(\.threads), in: 1...64)
                        .labelsHidden()
                        .accessibilityLabel("Fils de calcul")
                        .accessibilityValue("\(store.values.threads)")
                }
                SettingRow(title: "Sessions", detail: "0 : automatique") {
                    ValueText(store.values.sessions == 0 ? "Auto" : "\(store.values.sessions)", width: 40)
                    Stepper(Self.sessionsText(store.values.sessions), value: store.engineBinding(\.sessions), in: 0...16)
                        .labelsHidden()
                        .accessibilityLabel("Sessions")
                        .accessibilityValue(Self.sessionsText(store.values.sessions))
                }
                SettingRow(title: "Phrases préparées d'avance") {
                    ValueText("\(store.values.prefetch)", width: 30)
                    Stepper("Phrases préparées d'avance : \(store.values.prefetch)",
                            value: store.engineBinding(\.prefetch), in: 2...64)
                        .labelsHidden()
                        .accessibilityLabel("Phrases préparées d'avance")
                        .accessibilityValue("\(store.values.prefetch)")
                }
            }
        }
        .accessibilityIdentifier("onglet-avance")
        .onDisappear { store.endDragging() }
    }

    static func engineOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Automatique", "auto"), ("Voix du système", "system")],
                          current: current) { "\($0) (écrit à la main)" }
    }

    static func sessionsText(_ n: UInt32) -> String {
        n == 0 ? "Sessions : automatique" : "Sessions : \(n)"
    }
}

import SwiftUI

/// Onglet Lecture : l'affichage du texte et la façon de le parcourir.
struct ReadingTab: View {
    @Bindable var store: SettingsStore

    var body: some View {
        SettingsPage(title: "Lecture", subtitle: "L'affichage du texte et la façon de le parcourir.") {
            ErrorText(store.error, identifier: "erreur-reglages")
            SettingsGroup(title: "Texte") {
                SettingRow(title: "Police de lecture", detail: "Le texte lu, dans le panneau") {
                    PopUpPicker(label: "Police de lecture", identifier: "police-lecture",
                                options: ReadingFont.allCases.map { ($0.label, $0.rawValue) },
                                selection: Binding(get: { ReadingFont(rawValue: store.values.readingFont)?.rawValue
                                                              ?? ReadingFont.literata.rawValue },
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
                            value: count(\.lineSpacing), in: 0...3)
                        .labelsHidden()
                        .accessibilityLabel("Interlignage")
                        .accessibilityValue("\(store.values.lineSpacing)")
                }
                SettingRow(title: "Règle de lecture", detail: "Une bande sous la ligne lue, pour ne pas la perdre") {
                    Toggle("Règle de lecture", isOn: store.binding(\.readingRuler)).notionSwitch()
                }
            }
            SettingsGroup(title: "Parcours") {
                SettingRow(title: "Blocs de code", detail: "Ce que la voix fait d'un bloc de code") {
                    PopUpPicker(label: "Blocs de code", identifier: "blocs-code",
                                options: Self.codeOptions(current: store.values.codeBlocks),
                                selection: store.binding(\.codeBlocks))
                        .fixedSize()
                }
                SettingRow(title: "Lire dès l'ouverture", detail: "La voix part dès qu'un texte arrive") {
                    Toggle("Lire dès l'ouverture", isOn: store.binding(\.autoplay)).notionSwitch()
                }
                SettingRow(title: "Pas de l'avance rapide", detail: "Ce que sautent ⇧← et ⇧→") {
                    ValueText(Self.seekText(Int(store.values.seekStep)), width: 70)
                    Stepper("Pas de l'avance rapide : \(Self.seekText(Int(store.values.seekStep)))",
                            value: count(\.seekStep), in: 1...50)
                        .labelsHidden()
                        .accessibilityLabel("Pas de l'avance rapide")
                        .accessibilityValue(Self.seekText(Int(store.values.seekStep)))
                }
            }
        }
        .accessibilityIdentifier("onglet-lecture")
    }

    /// Le moteur compte en `UInt32`, un `Stepper` en `Int`.
    private func count(_ key: WritableKeyPath<SettingsView, UInt32>) -> Binding<Int> {
        Binding(get: { Int(store.values[keyPath: key]) },
                set: { store.set(key, UInt32(max(0, $0))) })
    }

    /// « 1 phrase », « 2 phrases » : le pluriel dès 2.
    static func seekText(_ n: Int) -> String { n == 1 ? "1 phrase" : "\(n) phrases" }

    static func codeOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Annoncer", "announce"), ("Sauter", "skip"), ("Lire", "read")],
                          current: current) { "\($0) (écrit à la main)" }
    }
}

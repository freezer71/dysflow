import SwiftUI

/// Onglet Lecture : l'affichage du texte et la façon de le parcourir.
struct ReadingTab: View {
    @Bindable var store: SettingsStore
    @AppStorage(DisplayDefaults.fontKey) private var fontName = ReadingFont.literata.rawValue
    @AppStorage(DisplayDefaults.sizeKey) private var size = DisplayDefaults.defaultSize

    var body: some View {
        Form {
            Section("Texte") {
                LabeledContent("Police de lecture") {
                    PopUpPicker(label: "Police de lecture", identifier: "police-lecture",
                                options: ReadingFont.allCases.map { ($0.label, $0.rawValue) },
                                selection: $fontName)
                        .fixedSize()
                }
                Slider(value: $size, in: 14...40, step: 1) { Text("Taille : \(Int(size)) pt") }
                    .accessibilityLabel("Taille du texte")
                    .accessibilityValue("\(Int(size)) points")
                Stepper("Interlignage : \(store.values.lineSpacing)",
                        value: count(\.lineSpacing), in: 0...3)
                    .accessibilityLabel("Interlignage")
                    .accessibilityValue("\(store.values.lineSpacing)")
                Toggle("Règle de lecture", isOn: store.binding(\.readingRuler))
            }
            Section("Lecture") {
                LabeledContent("Blocs de code") {
                    PopUpPicker(label: "Blocs de code", identifier: "blocs-code",
                                options: Self.codeOptions(current: store.values.codeBlocks),
                                selection: store.binding(\.codeBlocks))
                        .fixedSize()
                }
                Toggle("Lire dès l'ouverture", isOn: store.binding(\.autoplay))
                Stepper("Pas de l'avance rapide : \(store.values.seekStep) phrases",
                        value: count(\.seekStep), in: 1...50)
                    .accessibilityLabel("Pas de l'avance rapide")
                    .accessibilityValue("\(store.values.seekStep) phrases")
            }
            if let error = store.error { Text(error).foregroundStyle(.red) }
        }
        .formStyle(.grouped)
        .accessibilityIdentifier("onglet-lecture")
    }

    /// Le moteur compte en `UInt32`, un `Stepper` en `Int`.
    private func count(_ key: WritableKeyPath<SettingsView, UInt32>) -> Binding<Int> {
        Binding(get: { Int(store.values[keyPath: key]) },
                set: { store.set(key, UInt32(max(0, $0))) })
    }

    static func codeOptions(current: String) -> [(title: String, value: String)] {
        Choices.including([("Annoncer", "announce"), ("Sauter", "skip"), ("Lire", "read")],
                          current: current) { "\($0) (écrit à la main)" }
    }
}

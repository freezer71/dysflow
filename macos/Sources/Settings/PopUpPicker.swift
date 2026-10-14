import AppKit
import SwiftUI

/// Un menu local AppKit, pour les réglages.
///
/// Dans un formulaire groupé, le `Picker` de SwiftUI (macOS 26) n'offre à
/// VoiceOver que « appuyer » (AXPress), sans « afficher le menu »
/// (AXShowMenu) : l'audit d'accessibilité le signale. Le `NSPopUpButton`
/// offre les deux.
struct PopUpPicker<Value: Hashable>: NSViewRepresentable {
    let label: String
    let identifier: String
    let options: [(title: String, value: Value)]
    @Binding var selection: Value

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSPopUpButton {
        let button = NSPopUpButton(frame: .zero, pullsDown: false)
        button.font = .systemFont(ofSize: 13)
        button.target = context.coordinator
        button.action = #selector(Coordinator.changed(_:))
        button.setAccessibilityLabel(label)
        button.setAccessibilityIdentifier(identifier)
        // Sous une largeur imposée, le titre se tronque au lieu de déborder.
        button.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        (button.cell as? NSPopUpButtonCell)?.lineBreakMode = .byTruncatingTail
        return button
    }

    func updateNSView(_ button: NSPopUpButton, context: Context) {
        context.coordinator.parent = self
        let titles = options.map(\.title)
        if button.itemTitles != titles {
            button.removeAllItems()
            button.addItems(withTitles: titles)
        }
        if let index = options.firstIndex(where: { $0.value == selection }),
           button.indexOfSelectedItem != index {
            button.selectItem(at: index)
        }
    }

    @MainActor
    final class Coordinator: NSObject {
        var parent: PopUpPicker
        init(_ parent: PopUpPicker) { self.parent = parent }

        @objc func changed(_ sender: NSPopUpButton) {
            let index = sender.indexOfSelectedItem
            guard parent.options.indices.contains(index) else { return }
            parent.selection = parent.options[index].value
        }
    }
}

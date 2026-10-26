import AppKit
import SwiftUI

/// La fenêtre « Il manque des autorisations » : ce qui manque, pourquoi, et
/// un bouton par autorisation. Rien ne part tout seul : c'est le bouton qui
/// pose la question de macOS (le micro) ou ouvre le bon volet des Réglages
/// Système (l'Accessibilité). Une fenêtre ordinaire, jamais flottante : les
/// Réglages Système passent devant elle quand ils s'ouvrent.
@MainActor
final class PermissionsPrompt: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var onClose: (() -> Void)?

    /// Montre la fenêtre ; `onClose` vient à sa fermeture (Continuer, Plus
    /// tard ou la case de fermeture).
    func show(onClose: @escaping () -> Void) {
        self.onClose = onClose
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 480, height: 320),
                              styleMask: [.titled, .closable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.title = String(localized: "Permissions")
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.contentView = NSHostingView(rootView: PermissionsPromptView { [weak window] in window?.close() })
        window.center()
        self.window = window
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        let done = onClose
        onClose = nil
        window = nil
        done?()
    }
}

struct PermissionsPromptView: View {
    let close: () -> Void
    @State private var states: [Permission: Permission.State] = [:]
    /// L'Accessibilité a été demandée : macOS attend qu'on coche Dysflow.
    @State private var accessibilityOpened = false

    private var allGranted: Bool {
        Permission.relevant.allSatisfy { states[$0] == .granted }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                LogoMark().frame(width: 30, height: 30)
                Text(allGranted ? String(localized: "All set") : String(localized: "Permissions are missing"))
                    .font(Typo.pageTitle)
                    .foregroundStyle(Palette.ink)
                    .accessibilityAddTraits(.isHeader)
            }
            Text(allGranted
                 ? String(localized: "Dysflow can read and write for you.")
                 : String(localized: "Dysflow needs them to read what you select and write what you dictate. Everything stays on your Mac."))
                .font(Typo.body)
                .foregroundStyle(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 10)

            SettingsGroup(title: String(localized: "Permissions")) {
                ForEach(Permission.relevant, id: \.self) { permission in
                    SettingRow(title: permission.title, detail: permission.detail) {
                        if states[permission] == .granted {
                            Tag(text: String(localized: "Allowed"), tint: .green)
                                .accessibilityIdentifier(permission.identifier)
                        } else {
                            Button(String(localized: "Allow")) {
                                if permission == .accessibility { accessibilityOpened = true }
                                Task {
                                    await permission.request()
                                    reload()
                                }
                            }
                            .buttonStyle(.notionPrimary)
                            .accessibilityLabel("Allow: \(permission.title)")
                            .accessibilityIdentifier(permission.identifier)
                        }
                    }
                }
            }
            .environment(\.settingsGroupTitleHidden, true)

            if accessibilityOpened, states[.accessibility] != .granted {
                Text("Turn on Dysflow in System Settings, then come back here.")
                    .font(Typo.body)
                    .foregroundStyle(Palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 14)
            }

            HStack(spacing: 10) {
                Spacer()
                if !allGranted {
                    Button(String(localized: "Later"), action: close)
                        .buttonStyle(.notion)
                        .keyboardShortcut(.cancelAction)
                }
                Button(String(localized: "Continue"), action: close)
                    .buttonStyle(.notionPrimary)
                    .disabled(!allGranted)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(.top, 22)
        }
        .padding(.horizontal, 28)
        .padding(.top, 34)
        .padding(.bottom, 24)
        .frame(width: 480)
        .fixedSize(horizontal: false, vertical: true)
        .background(Palette.page)
        // Relu à la seconde : une case cochée dans les Réglages Système se voit
        // ici sans rien faire. Quand tout est accordé, la fenêtre revient devant.
        .task {
            while !Task.isCancelled {
                let before = allGranted
                reload()
                if !before, allGranted, !states.isEmpty {
                    NSApp.activate()
                    NSApp.windows.first { $0.contentView is NSHostingView<PermissionsPromptView> }?
                        .makeKeyAndOrderFront(nil)
                }
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    private func reload() {
        var next: [Permission: Permission.State] = [:]
        for permission in Permission.relevant { next[permission] = permission.state }
        if next != states { states = next }
    }
}

import AppKit
import ApplicationServices
import AVFoundation

/// Les autorisations que sillon demande à macOS : l'Accessibilité (lire la
/// sélection, saisir la dictée) et le micro (la dictée).
enum Permission: CaseIterable {
    case accessibility
    case microphone

    enum State: Equatable {
        case granted
        /// Jamais demandée : macOS peut encore poser la question.
        case notAsked
        /// Refusée : seuls les Réglages Système peuvent la rendre.
        case denied
    }

    var title: String {
        switch self {
        case .accessibility: "Accessibilité"
        case .microphone: "Micro"
        }
    }

    var detail: String {
        switch self {
        case .accessibility: "Pour lire la sélection et saisir la dictée là où vous écrivez"
        case .microphone: "Pour la dictée ; la voix est transcrite sur votre Mac"
        }
    }

    var identifier: String {
        switch self {
        case .accessibility: "autorisation-accessibilite"
        case .microphone: "autorisation-micro"
        }
    }

    /// Le volet des Réglages Système où l'autoriser.
    var settingsURL: URL {
        switch self {
        case .accessibility:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        case .microphone:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!
        }
    }

    var state: State {
        switch self {
        case .accessibility:
            // Le ⌘V de la dictée demande aussi le droit d'envoyer des touches.
            return AXIsProcessTrusted() && CGPreflightPostEventAccess() ? .granted : .notAsked
        case .microphone:
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .authorized: return .granted
            case .notDetermined: return .notAsked
            default: return .denied
            }
        }
    }

    /// Les autorisations à proposer ici : le micro seulement avec la dictée.
    static var relevant: [Permission] {
        Dictation.isSupported ? allCases : [.accessibility]
    }

    /// Demande l'autorisation : la question de macOS si elle peut encore
    /// être posée, sinon le bon volet des Réglages Système.
    @MainActor
    func request() async {
        switch self {
        case .accessibility:
            // Une case cochée pour une autre construction de sillon ne vaut
            // pas pour celle-ci : elle s'efface, puis macOS repose sa question.
            if !AXIsProcessTrusted() { Self.reset("Accessibility") }
            if !CGPreflightPostEventAccess() { Self.reset("PostEvent") }
            // Inscrit sillon dans la liste et ouvre le volet : il n'y a plus
            // qu'à cocher.
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)
            // Ici, à la demande, plutôt qu'au premier ⌘V de la dictée.
            if !CGPreflightPostEventAccess() { _ = CGRequestPostEventAccess() }
            NSWorkspace.shared.open(settingsURL)
        case .microphone:
            // Refusé : effacé, macOS peut reposer sa question.
            if state == .denied { Self.reset("Microphone") }
            if state == .notAsked {
                NSApp.activate()
                if await AVCaptureDevice.requestAccess(for: .audio) { return }
            }
            NSWorkspace.shared.open(settingsURL)
        }
    }

    /// Efface ce que macOS a retenu pour sillon, et pour lui seul, sur une
    /// autorisation (`tccutil reset`).
    private static func reset(_ service: String) {
        guard let bundle = Bundle.main.bundleIdentifier else { return }
        let tool = Process()
        tool.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        tool.arguments = ["reset", service, bundle]
        tool.standardOutput = FileHandle.nullDevice
        tool.standardError = FileHandle.nullDevice
        try? tool.run()
        tool.waitUntilExit()
    }
}

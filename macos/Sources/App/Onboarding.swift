import AppKit
import ApplicationServices

/// Au lancement, s'il manque une autorisation, la fenêtre « Il manque des
/// autorisations » l'explique et donne un bouton par autorisation. Rien n'est
/// demandé à macOS sans ce clic.
///
/// Une fois par version : une nouvelle signature fait oublier à macOS ce qui
/// était accordé. Fermée sans tout accorder, la fenêtre ne revient pas avant
/// la version suivante ; le menu le rappelle, les réglages y mènent.
@MainActor
enum Onboarding {
    private static let askedKey = "autorisationsDemandeesPourLaVersion"
    private static let remindedKey = "accessibilityReminded"

    private static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }

    /// La fenêtre est à montrer à ce lancement.
    static var shouldPrompt: Bool {
        UserDefaults.standard.string(forKey: askedKey) != version
            && Permission.relevant.contains { $0.state != .granted }
    }

    static func prompted() { UserDefaults.standard.set(version, forKey: askedKey) }

    /// Le menu le rappelle une fois, discrètement.
    static var needsReminder: Bool {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: remindedKey) else { return false }
        return true
    }

    static func reminded() { UserDefaults.standard.set(true, forKey: remindedKey) }
}

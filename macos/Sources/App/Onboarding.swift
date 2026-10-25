import AppKit
import ApplicationServices
import AVFoundation

/// Au lancement, demander d'emblée ce qui manque : le micro (la question de
/// macOS), puis l'Accessibilité (macOS inscrit Dysflow et ouvre son volet).
/// Sans cela, il fallait aller dans les réglages, ou lancer une dictée.
///
/// Une fois par version : une nouvelle signature fait oublier à macOS ce qui
/// était accordé, et la question revient d'elle-même. Mais un refus n'est pas
/// reposé à chaque lancement ; le menu le rappelle, les réglages y mènent.
@MainActor
enum Onboarding {
    private static let askedKey = "autorisationsDemandeesPourLaVersion"
    private static let remindedKey = "accessibilityReminded"

    static func runIfNeeded() async {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        let defaults = UserDefaults.standard
        guard defaults.string(forKey: askedKey) != version else { return }
        let missing = Permission.relevant.filter { $0.state != .granted }
        guard !missing.isEmpty else { return }
        defaults.set(version, forKey: askedKey)
        // Le micro d'abord : sa question est une fenêtre de macOS qu'on attend.
        // Refusé une fois, il n'est plus redemandé ici.
        if missing.contains(.microphone), Permission.microphone.state == .notAsked {
            NSApp.activate()
            _ = await AVCaptureDevice.requestAccess(for: .audio)
        }
        if missing.contains(.accessibility) {
            await Permission.accessibility.request()
        }
    }

    /// Le menu le rappelle une fois, discrètement.
    static var needsReminder: Bool {
        guard !AXIsProcessTrusted(), !UserDefaults.standard.bool(forKey: remindedKey) else { return false }
        return true
    }

    static func reminded() { UserDefaults.standard.set(true, forKey: remindedKey) }
}

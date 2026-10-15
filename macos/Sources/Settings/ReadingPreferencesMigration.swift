import Foundation

/// Police et taille vivaient dans les préférences de l'app ; elles vivent
/// désormais dans config.toml. Au lancement, ce qui reste des préférences y
/// est écrit, puis effacé — seulement si l'écriture a eu lieu.
@MainActor
enum ReadingPreferencesMigration {
    static let fontKey = "readingFont"
    static let sizeKey = "readingSize"

    /// Le patch qui reprend les préférences : une police connue telle
    /// quelle, une taille bornée à 14–40 ; nil s'il n'y a rien à reprendre.
    static func patch(from defaults: UserDefaults) -> SettingsPatch? {
        var patch = SettingsPatch.empty
        if let font = defaults.string(forKey: fontKey), ReadingFont(rawValue: font) != nil {
            patch.readingFont = font
        }
        if defaults.object(forKey: sizeKey) != nil {
            let size = defaults.double(forKey: sizeKey)
            if size.isFinite, size > 0 { patch.readingSize = Float(min(max(size, 14), 40)) }
        }
        return patch.isEmpty ? nil : patch
    }

    /// Vrai si des valeurs ont été écrites. config.toml illisible : le
    /// moteur n'y écrit rien, les préférences attendent le lancement suivant.
    @discardableResult
    static func run(defaults: UserDefaults, backend: SettingsBackend, configUnreadable: Bool) async -> Bool {
        guard let patch = patch(from: defaults) else {
            forget(defaults)
            return false
        }
        guard !configUnreadable else { return false }
        do {
            _ = try await backend.updateSettings(patch)
        } catch {
            return false
        }
        forget(defaults)
        return true
    }

    private static func forget(_ defaults: UserDefaults) {
        defaults.removeObject(forKey: fontKey)
        defaults.removeObject(forKey: sizeKey)
    }
}

import Foundation
import MachO

/// La première synthèse après une installation ou une mise à jour compile les
/// noyaux Metal (≈ 9 s), gardés ensuite dans le cache du système. L'app la
/// fait hors du fil principal, une fois par version, dès que Kokoro devient la
/// voix active (lancement, choix de la voix, fin du téléchargement accepté),
/// et libère le modèle aussitôt : les 9 s ne tombent jamais sur le premier
/// mot lu.
@MainActor
enum KokoroWarmUp {
    static let versionKey = "kokoroPreparedVersion"
    /// Vrai pendant la préparation : l'onglet Voix dit « Voix en préparation
    /// (première fois)… ».
    private(set) static var isRunning = false

    /// La version de l'app pour la préparation : `CFBundleShortVersionString`
    /// et `CFBundleVersion` (MARKETING_VERSION, CURRENT_PROJECT_VERSION). En
    /// développement, ou sans numéro de build, l'UUID de l'exécutable
    /// (LC_UUID) s'y ajoute : il change avec chaque binaire différent.
    nonisolated static func version(of bundle: Bundle) -> String {
        #if DEBUG
        let development = true
        #else
        let development = false
        #endif
        let info = bundle.infoDictionary ?? [:]
        return version(short: info["CFBundleShortVersionString"] as? String,
                       build: info["CFBundleVersion"] as? String,
                       uuid: executableUUID(of: bundle), development: development)
    }

    nonisolated static func version(short: String?, build: String?, uuid: String?,
                                    development: Bool) -> String {
        let numbers = "\(short ?? "?")(\(build ?? "?"))"
        let hasBuild = !(build ?? "").isEmpty
        guard development || !hasBuild else { return numbers }
        return "\(numbers)-\(uuid ?? "?")"
    }

    /// L'UUID (LC_UUID) de l'exécutable du paquet, lu dans l'image chargée.
    nonisolated static func executableUUID(of bundle: Bundle) -> String? {
        guard let path = bundle.executableURL?.resolvingSymlinksInPath().path else { return nil }
        for i in 0..<_dyld_image_count() {
            guard let name = _dyld_get_image_name(i),
                  URL(fileURLWithPath: String(cString: name)).resolvingSymlinksInPath().path == path,
                  let header = _dyld_get_image_header(i),
                  header.pointee.magic == MH_MAGIC_64 else { continue }
            var command = UnsafeRawPointer(header).advanced(by: MemoryLayout<mach_header_64>.size)
            for _ in 0..<header.pointee.ncmds {
                let load = command.load(as: load_command.self)
                if load.cmd == UInt32(LC_UUID) {
                    let uuid = command.load(as: uuid_command.self).uuid
                    return UUID(uuid: uuid).uuidString
                }
                command = command.advanced(by: Int(load.cmdsize))
            }
        }
        return nil
    }

    /// Le dossier du paquet MLX actif ; aucun pour un autre modèle.
    static func modelDirectory(settings: SettingsView) -> String? {
        guard settings.family == "kokoro", settings.modelPath.hasSuffix(".safetensors") else { return nil }
        return URL(fileURLWithPath: settings.modelPath).deletingLastPathComponent().path
    }

    @discardableResult
    static func runIfNeeded(compute: KokoroCompute, modelDirectory: String?, version: String,
                            defaults: UserDefaults = .standard) -> Task<Void, Never>? {
        guard let directory = modelDirectory, !isRunning,
              defaults.string(forKey: versionKey) != version else { return nil }
        isRunning = true
        // `.utility` et non `.background` : le lecteur peut attendre le verrou
        // MLX derrière elle, et une préparation bridée le ferait attendre.
        let work = Task.detached(priority: .utility) { () -> Bool in
            do {
                try compute.load(modelDir: directory)
                defer { compute.release() }
                try compute.warmUp()
                return true
            } catch {
                return false
            }
        }
        return Task { @MainActor in
            let prepared = await work.value
            isRunning = false
            if prepared { defaults.set(version, forKey: versionKey) }
        }
    }
}

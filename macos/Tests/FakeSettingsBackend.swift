@testable import Sillon

/// Capacités relevées sur `models::honoured_options`.
enum Capabilities {
    static let supertonic = ModelCapabilities(
        family: "supertonic",
        voices: [VoiceChoice(id: "M1", label: "M1 · masculine 1", language: ""),
                 VoiceChoice(id: "F2", label: "F2 · féminine 2", language: "")],
        languages: ["fr", "en"],
        honoured: ["voice", "language", "speaking_rate", "seed", "num_inference_steps"],
        requiresReference: false)
    static let magpie = ModelCapabilities(
        family: "magpie_tts",
        voices: [VoiceChoice(id: "Aria", label: "Aria", language: "")],
        languages: ["fr", "en", "de"],
        honoured: ["voice", "voice_id", "speaker", "language", "temperature", "top_k",
                   "guidance_scale", "seed"],
        requiresReference: false)
    static let kokoro = ModelCapabilities(
        family: "kokoro",
        voices: [VoiceChoice(id: "p-fenrir60", label: "p-fenrir60 · française, recommandée", language: "fr"),
                 VoiceChoice(id: "af_bella", label: "af_bella · américaine, femme, recommandée", language: "en"),
                 VoiceChoice(id: "am_fenrir", label: "am_fenrir · américaine, homme", language: "en"),
                 VoiceChoice(id: "af_heart", label: "af_heart · américaine, femme", language: "en")],
        languages: ["fr", "en"],
        honoured: ["voice", "language"],
        requiresReference: false)
}

/// Un moteur pour les tests : il garde les patchs, borne l'interlignage
/// comme le vrai, et n'écrit rien nulle part.
@MainActor
final class FakeSettingsBackend: SettingsBackend {
    var stored = SettingsView.defaults
    var capabilities = Capabilities.supertonic
    private(set) var patches: [SettingsPatch] = []
    private(set) var log: [String] = []
    /// Message de l'erreur rendue par la prochaine écriture.
    var failNext: String?

    func loadSettings() async -> SettingsView { stored }
    func loadCapabilities() async -> ModelCapabilities { capabilities }

    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView {
        patches.append(patch)
        log.append("patch")
        if let failNext {
            self.failNext = nil
            throw SillonError.Failed(message: failNext)
        }
        if let v = patch.voice { stored.voice = v }
        if let v = patch.speed { stored.speed = v }
        if let v = patch.volume { stored.volume = v }
        if let v = patch.threads { stored.threads = v }
        if let v = patch.lineSpacing { stored.lineSpacing = min(v, 3) }
        if let v = patch.codeBlocks { stored.codeBlocks = v }
        if let v = patch.voiceForLanguage {
            stored.voicesByLanguage.removeAll { $0.language == v.language }
            stored.voicesByLanguage.append(v)
        }
        return stored
    }

    /// Refus rendu par chaque « Écouter ».
    var failPreview: String?

    func previewVoice(in language: String) async throws {
        log.append("écoute \(language)")
        if let failPreview { throw SillonError.Failed(message: failPreview) }
    }
    func stopPreview() async { log.append("arrêt") }
}

/// Un moteur dont les lectures et les écritures attendent qu'on les libère :
/// pour provoquer les courses (changement pendant une relecture, écritures
/// qui se chevauchent).
@MainActor
final class GatedSettingsBackend: SettingsBackend {
    var stored = SettingsView.defaults
    var gateLoads = false
    var gateWrites = false
    private(set) var patches: [SettingsPatch] = []
    private(set) var running = 0
    private(set) var maxRunning = 0
    private var loadGates: [CheckedContinuation<Void, Never>] = []
    private var writeGates: [CheckedContinuation<Void, Never>] = []
    var waitingLoads: Int { loadGates.count }
    var waitingWrites: Int { writeGates.count }

    func releaseLoad() { if !loadGates.isEmpty { loadGates.removeFirst().resume() } }
    func releaseWrite() { if !writeGates.isEmpty { writeGates.removeFirst().resume() } }

    /// Libère tout ce qui attend et ouvre les portes : un test qui échoue
    /// ne laisse jamais une tâche bloquée.
    func releaseAll() {
        gateLoads = false
        gateWrites = false
        let l = loadGates, w = writeGates
        loadGates = []
        writeGates = []
        (l + w).forEach { $0.resume() }
    }

    func loadSettings() async -> SettingsView {
        // La valeur lue l'est à l'appel, pas à la libération de la porte.
        let snapshot = stored
        if gateLoads { await withCheckedContinuation { loadGates.append($0) } }
        return snapshot
    }
    func loadCapabilities() async -> ModelCapabilities { Capabilities.supertonic }

    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView {
        running += 1
        maxRunning = max(maxRunning, running)
        patches.append(patch)
        if gateWrites { await withCheckedContinuation { writeGates.append($0) } }
        if let v = patch.voice { stored.voice = v }
        if let v = patch.speed { stored.speed = v }
        if let v = patch.language { stored.language = v }
        running -= 1
        return stored
    }
    func previewVoice(in language: String) async throws {}
    func stopPreview() async {}
}

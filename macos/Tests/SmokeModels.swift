import XCTest

/// Le dossier d'un modèle de dictée sous `SILLAGE_STT_MODELS`, sinon le
/// test est ignoré (aussi quand ce modèle-là n'y est pas : on n'a pas
/// forcément tous les modèles).
enum SmokeModels {
    static func folder(_ id: String) throws -> URL {
        guard let root = ProcessInfo.processInfo.environment["SILLAGE_STT_MODELS"] else {
            throw XCTSkip("SILLAGE_STT_MODELS absent")
        }
        let folder = URL(fileURLWithPath: root).appendingPathComponent(id)
        guard FileManager.default.fileExists(atPath: folder.appendingPathComponent("model.safetensors").path) else {
            throw XCTSkip("\(id) absent de SILLAGE_STT_MODELS")
        }
        return folder
    }
}

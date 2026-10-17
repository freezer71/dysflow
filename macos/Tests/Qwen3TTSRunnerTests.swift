import XCTest
@testable import KokoroMLX

/// La porte de Qwen3-TTS, sans modèle : langue, dialecte, graine, et un
/// chargeur qui ne touche jamais au dossier du modèle.
final class Qwen3TTSRunnerTests: XCTestCase {
    func testLanguagesAreThoseOfTheCodec() {
        XCTAssertEqual(Qwen3TTSRunner.languageName("fr"), "french")
        XCTAssertEqual(Qwen3TTSRunner.languageName("fr-FR"), "french")
        XCTAssertEqual(Qwen3TTSRunner.languageName("en"), "english")
        XCTAssertEqual(Qwen3TTSRunner.languageName("de"), "auto")
    }

    /// Bogue du port (relevé §1.f) : le dialecte d'Eric ou de Dylan ne
    /// remplace la langue que pour `chinese` ou `auto`, comme l'officiel.
    func testADialectSpeakerKeepsTheRequestedLanguage() {
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "french", dialect: "sichuan_dialect"), "french")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "english", dialect: "beijing_dialect"), "english")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "chinese", dialect: "sichuan_dialect"), "sichuan_dialect")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "auto", dialect: "beijing_dialect"), "beijing_dialect")
        XCTAssertEqual(Qwen3TTSModel.effectiveLanguage(requested: "french", dialect: nil), "french")
    }

    /// Même phrase, même graine, d'un lancement à l'autre (`hashValue`
    /// change à chaque processus : interdit ici).
    func testTheSeedIsStableAndIgnoresUnicodeForm() {
        XCTAssertEqual(StableSeed.of("Bonjour."), 7_611_860_131_407_550_282)
        XCTAssertEqual(StableSeed.of(""), 14_695_981_039_346_656_037)
        XCTAssertEqual(StableSeed.of("e\u{301}"), StableSeed.of("é"))
        XCTAssertEqual(StableSeed.of("é"), 775_207_407_765_167_617)
    }

    /// Relevé §1.e : un `speech_tokenizer` qui n'est pas un dossier faisait
    /// effacer le dossier du modèle. Ici : refusé avant toute lecture, rien
    /// d'écrit, rien d'effacé.
    func testAMalformedFolderIsRefusedAndLeftUntouched() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("qwen3-intact-\(UUID())")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        for name in ["config.json", "model.safetensors", "vocab.json", "merges.txt", "tokenizer_config.json", "speech_tokenizer"] {
            try Data("x".utf8).write(to: dir.appendingPathComponent(name))
        }
        let before = try FileManager.default.contentsOfDirectory(atPath: dir.path).sorted()
        XCTAssertThrowsError(try Qwen3TTSRunner.load(directory: dir))
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.path), "le dossier n'est jamais effacé")
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: dir.path).sorted(), before,
                       "pas de tokenizer.json écrit")
    }

    func testTheFixedFrenchVoiceShipsInTheFramework() {
        XCTAssertNotNil(Bundle(for: Qwen3TTSRunner.self).url(forResource: "qwen3-06b-siwis", withExtension: "safetensors"))
    }
}

import XCTest
@testable import Sillon

final class MLXBudgetTests: XCTestCase {
    final class Recorder: @unchecked Sendable {
        var applied: [(Int, Int)] = []
    }

    func budget() -> (MLXBudget, Recorder) {
        let r = Recorder()
        return (MLXBudget(apply: { r.applied.append(($0, $1)) }), r)
    }

    func testTheLimitsAreTheSumOfTheLoadedModels() {
        let (b, r) = budget()
        b.set("kokoro", .init(memory: 1 << 30, cache: 64 << 20))
        b.set("dictee", .init(memory: 3 << 30, cache: 256 << 20))
        XCTAssertEqual(b.total, .init(memory: 4 << 30, cache: 320 << 20))
        XCTAssertEqual(r.applied.last?.0, 4 << 30)
        b.set("kokoro", .init(memory: 2 << 30, cache: 64 << 20))
        XCTAssertEqual(b.total.memory, 5 << 30, "une clé remplace son besoin, ne l'ajoute pas deux fois")
    }

    /// Spec (« Mémoire ») et Review Focus 5 : une dictée libérée pendant
    /// que Qwen 1.7B lit ne lui impose plus 1 Go.
    func testReleasingDictationKeepsTheReadingModelsNeed() {
        let (b, r) = budget()
        b.set("lecture:/m/q17", .init(memory: 4 << 30, cache: 256 << 20))
        b.set("dictee", .init(memory: 5 << 30, cache: 256 << 20))
        b.remove("dictee")
        XCTAssertEqual(r.applied.last?.0, 4 << 30)
        XCTAssertEqual(r.applied.last?.1, 256 << 20)
    }

    func testNothingLoadedAppliesNothing() {
        let (b, r) = budget()
        b.set("kokoro", .init(memory: 1 << 30, cache: 64 << 20))
        let count = r.applied.count
        b.remove("kokoro")
        b.remove("inconnue")
        XCTAssertEqual(r.applied.count, count, "rien de chargé : plafonds laissés tels quels")
        XCTAssertEqual(b.total, .init(memory: 0, cache: 0))
    }

    func testWeightBytesCountSubfolders() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("budget-\(UUID())")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("speech_tokenizer"),
                                                withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try Data(count: 10).write(to: dir.appendingPathComponent("model.safetensors"))
        try Data(count: 5).write(to: dir.appendingPathComponent("speech_tokenizer/model.safetensors"))
        try Data(count: 7).write(to: dir.appendingPathComponent("vocab.json"))
        XCTAssertEqual(MLXBudget.weightBytes(dir), 15)
    }
}

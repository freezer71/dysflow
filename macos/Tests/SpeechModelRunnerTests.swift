import XCTest
@testable import Sillon

final class SpeechModelRunnerTests: XCTestCase {
    final class Echo: LoadedSpeechModel {
        func transcribe(_ samples: [Float], language: String) throws -> String { "\(samples.count) \(language)" }
    }

    /// Les vidages du cache MLX demandés : jamais le vrai dans les tests.
    final class Clears: @unchecked Sendable {
        private let lock = NSLock()
        private var value = 0
        var count: Int { lock.withLock { value } }
        func clear() { lock.withLock { value += 1 } }
    }

    func testLoadsOnceThenTranscribes() async throws {
        var loads = 0
        let clears = Clears()
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() }, clearCache: clears.clear)
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        let text = try await runner.transcribe([0, 0, 0], language: "fr")
        XCTAssertEqual(text, "3 fr")
        XCTAssertEqual(loads, 1)
        XCTAssertEqual(clears.count, 0)
    }

    /// Un autre dossier : l'ancien modèle est libéré (cache vidé une fois),
    /// puis le nouveau chargé.
    func testAnotherDirectoryReloads() async throws {
        var loads = 0
        let clears = Clears()
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() }, clearCache: clears.clear)
        try await runner.prepare(directory: URL(fileURLWithPath: "/a"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/b"))
        XCTAssertEqual(loads, 2)
        XCTAssertEqual(clears.count, 1)
    }

    func testTranscribingBeforePreparingFails() async {
        let runner = SpeechModelRunner(load: { _ in Echo() }, clearCache: {})
        do { _ = try await runner.transcribe([], language: "fr"); XCTFail() } catch {}
    }

    /// Sans usage pendant `releaseAfter`, le modèle part tout seul (cache
    /// vidé) ; transcrire échoue ensuite.
    func testTheModelIsReleasedWhenIdle() async throws {
        let released = expectation(description: "libéré")
        let runner = SpeechModelRunner(load: { _ in Echo() }, releaseAfter: .milliseconds(50),
                                       clearCache: { released.fulfill() })
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        await fulfillment(of: [released], timeout: 5)
        do { _ = try await runner.transcribe([], language: "fr"); XCTFail("libéré") } catch {}
    }
}

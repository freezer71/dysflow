import XCTest
@testable import Sillage

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

    /// Une dictée en cours note son usage entre deux relectures
    /// (`keepAlive`) : le modèle reste tant qu'elle dure, si espacées que
    /// soient ses lectures, puis part le délai écoulé.
    func testKeepAliveHoldsTheModelThenItGoes() async throws {
        let clears = Clears()
        let runner = SpeechModelRunner(load: { _ in Echo() }, releaseAfter: .milliseconds(150),
                                       clearCache: clears.clear)
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        for _ in 0..<10 {
            try await Task.sleep(for: .milliseconds(50))
            runner.keepAlive()
        }
        XCTAssertEqual(clears.count, 0, "libéré pendant la dictée")
        XCTAssertNotNil(runner.loadedDirectory)
        for _ in 0..<100 where clears.count == 0 {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertEqual(clears.count, 1)
        XCTAssertNil(runner.loadedDirectory)
    }

    /// `keep_voice_minutes`, comme la voix : négatif, jamais ; 0, peu après
    /// (jamais entre deux relectures) ; sinon ces minutes.
    func testTheDelayFollowsKeepVoiceMinutes() {
        XCTAssertNil(SpeechModelRunner.idleDelay(keepVoiceMinutes: -1))
        XCTAssertEqual(SpeechModelRunner.idleDelay(keepVoiceMinutes: 0), SpeechModelRunner.shortestDelay)
        XCTAssertEqual(SpeechModelRunner.idleDelay(keepVoiceMinutes: 3), .seconds(180))
    }

    /// Le délai change en route : le suivant usage le prend.
    func testTheDelayCanChange() async throws {
        let released = expectation(description: "libéré")
        let runner = SpeechModelRunner(load: { _ in Echo() }, releaseAfter: nil,
                                       clearCache: { released.fulfill() })
        XCTAssertNil(runner.releaseAfter)
        runner.releaseAfter = .milliseconds(50)
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        await fulfillment(of: [released], timeout: 5)
    }

    /// `release` (le modèle choisi supprimé) : le modèle part tout de suite.
    func testReleaseUnloadsAtOnce() async throws {
        let clears = Clears()
        let runner = SpeechModelRunner(load: { _ in Echo() }, clearCache: clears.clear)
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        runner.release()
        XCTAssertEqual(clears.count, 1)
        do { _ = try await runner.transcribe([], language: "fr"); XCTFail("libéré") } catch {}
    }
    /// Le temps passé à calculer (sous le verrou), pour le débit de l'essai
    /// du micro : il grandit à chaque transcription, jamais au chargement.
    func testComputeTimeAddsUpTranscriptionsOnly() async throws {
        final class Slow: LoadedSpeechModel {
            func transcribe(_ samples: [Float], language: String) throws -> String {
                Thread.sleep(forTimeInterval: 0.05)
                return ""
            }
        }
        let runner = SpeechModelRunner(load: { _ in Thread.sleep(forTimeInterval: 0.2); return Slow() },
                                       clearCache: {})
        XCTAssertEqual(runner.computeTime, .zero)
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        XCTAssertEqual(runner.computeTime, .zero, "le chargement n'est pas du calcul")
        _ = try await runner.transcribe([], language: "fr")
        _ = try await runner.transcribe([], language: "fr")
        XCTAssertGreaterThanOrEqual(runner.computeTime, .milliseconds(100))
        XCTAssertLessThan(runner.computeTime, .milliseconds(190))
    }

    /// Le dossier du modèle chargé : l'essai du micro sait s'il doit
    /// libérer un autre modèle avant de mesurer la mémoire du sien.
    func testTheLoadedDirectoryIsKnown() async throws {
        let runner = SpeechModelRunner(load: { _ in Echo() }, clearCache: {})
        XCTAssertNil(runner.loadedDirectory)
        try await runner.prepare(directory: URL(fileURLWithPath: "/a"))
        XCTAssertEqual(runner.loadedDirectory, URL(fileURLWithPath: "/a"))
        runner.release()
        XCTAssertNil(runner.loadedDirectory)
    }
    /// La fin d'un essai du micro libère son modèle, mais jamais celui
    /// qu'une dictée a chargé entre-temps.
    func testReleaseIfLoadedOnlyUnloadsThatDirectory() async throws {
        let clears = Clears()
        let runner = SpeechModelRunner(load: { _ in Echo() }, clearCache: clears.clear)
        try await runner.prepare(directory: URL(fileURLWithPath: "/a"))
        runner.release(ifLoaded: URL(fileURLWithPath: "/b"))
        XCTAssertEqual(runner.loadedDirectory, URL(fileURLWithPath: "/a"))
        XCTAssertEqual(clears.count, 0)
        runner.release(ifLoaded: URL(fileURLWithPath: "/a"))
        XCTAssertNil(runner.loadedDirectory)
        XCTAssertEqual(clears.count, 1)
    }
}

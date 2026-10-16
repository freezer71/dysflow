import XCTest
@testable import Sillon

final class SpeechModelRunnerTests: XCTestCase {
    final class Echo: LoadedSpeechModel {
        func transcribe(_ samples: [Float], language: String) throws -> String { "\(samples.count) \(language)" }
    }

    func testLoadsOnceThenTranscribes() async throws {
        var loads = 0
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() })
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/x"))
        let text = try await runner.transcribe([0, 0, 0], language: "fr")
        XCTAssertEqual(text, "3 fr")
        XCTAssertEqual(loads, 1)
    }

    func testAnotherDirectoryReloads() async throws {
        var loads = 0
        let runner = SpeechModelRunner(load: { _ in loads += 1; return Echo() })
        try await runner.prepare(directory: URL(fileURLWithPath: "/a"))
        try await runner.prepare(directory: URL(fileURLWithPath: "/b"))
        XCTAssertEqual(loads, 2)
    }

    func testTranscribingBeforePreparingFails() async {
        let runner = SpeechModelRunner(load: { _ in Echo() })
        do { _ = try await runner.transcribe([], language: "fr"); XCTFail() } catch {}
    }
}

import XCTest
@testable import Sillon

final class SettingsDiffTests: XCTestCase {
    func testNoChangeMakesAnEmptyPatch() {
        XCTAssertTrue(SettingsDiff.patch(from: .defaults, to: .defaults).isEmpty)
    }

    func testOnlyWhatChangesIsInThePatch() {
        var old = SettingsView.defaults
        old.seed = 42
        old.temperature = 0.7
        old.extra = [ExtraOption(key: "a", value: "1"), ExtraOption(key: "b", value: "2")]
        var new = old
        new.topK = 40
        new.seed = nil
        new.temperature = nil
        new.codeBlocks = "skip"
        new.extra = [ExtraOption(key: "a", value: "9"), ExtraOption(key: "c", value: "3")]
        let p = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(p.topK, .value(value: 40))
        XCTAssertEqual(p.seed, .random)
        XCTAssertEqual(p.temperature, .model, "rendu au modèle")
        XCTAssertEqual(p.codeBlocks, "skip")
        XCTAssertEqual(p.extraSet, [ExtraOption(key: "a", value: "9"), ExtraOption(key: "c", value: "3")])
        XCTAssertEqual(p.extraRemove, ["b"])
        XCTAssertNil(p.speed)
        XCTAssertNil(p.voice)
        XCTAssertNil(p.guidanceScale)
    }

    func testEveryScalarFieldIsCovered() {
        var new = SettingsView.defaults
        new.voice = "F1"; new.language = "en"; new.speed = 2; new.pitch = 0.5; new.energy = 1.5
        new.emotion = "calme"; new.instruction = "posé"; new.volume = 0.5; new.engine = "system"
        new.compute = "cpu"; new.threads = 2; new.sessions = 3; new.prefetch = 9
        new.topP = 0.9; new.guidanceScale = 3; new.repetitionPenalty = 1.2; new.inferenceSteps = 12
        new.autoplay = false; new.seekStep = 9; new.lineWidth = 80; new.lineSpacing = 2
        new.readingRuler = false; new.keepVoiceMinutes = 30
        let p = SettingsDiff.patch(from: .defaults, to: new)
        XCTAssertEqual(p.voice, "F1"); XCTAssertEqual(p.language, "en"); XCTAssertEqual(p.speed, 2)
        XCTAssertEqual(p.pitch, 0.5); XCTAssertEqual(p.energy, 1.5); XCTAssertEqual(p.emotion, "calme")
        XCTAssertEqual(p.instruction, "posé"); XCTAssertEqual(p.volume, 0.5); XCTAssertEqual(p.engine, "system")
        XCTAssertEqual(p.compute, "cpu"); XCTAssertEqual(p.threads, 2); XCTAssertEqual(p.sessions, 3)
        XCTAssertEqual(p.prefetch, 9); XCTAssertEqual(p.topP, .value(value: 0.9))
        XCTAssertEqual(p.guidanceScale, .value(value: 3)); XCTAssertEqual(p.repetitionPenalty, .value(value: 1.2))
        XCTAssertEqual(p.inferenceSteps, .value(value: 12)); XCTAssertEqual(p.autoplay, false)
        XCTAssertEqual(p.seekStep, 9); XCTAssertEqual(p.lineWidth, 80); XCTAssertEqual(p.lineSpacing, 2)
        XCTAssertEqual(p.readingRuler, false); XCTAssertEqual(p.keepVoiceMinutes, 30)
    }

    func testAnUnlistedValueStaysInTheMenu() {
        let options: [(title: String, value: String)] = [("Annoncer", "announce"), ("Sauter", "skip")]
        let kept = Choices.including(options, current: "lire") { "\($0) (écrit à la main)" }
        XCTAssertEqual(kept.map(\.value), ["announce", "skip", "lire"])
        XCTAssertEqual(kept.last?.title, "lire (écrit à la main)")
        XCTAssertEqual(Choices.including(options, current: "skip") { $0 }.map(\.value), ["announce", "skip"])
    }

    func testAnUnknownFamilyIgnoresNothing() {
        XCTAssertFalse(SettingKey.temperature.isIgnored(honoured: []))
        XCTAssertTrue(SettingKey.temperature.isIgnored(honoured: ["voice", "speaking_rate"]))
        XCTAssertFalse(SettingKey.speed.isIgnored(honoured: ["voice", "speaking_rate"]))
    }
}

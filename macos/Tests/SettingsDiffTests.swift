import XCTest
@testable import Sillon

final class SettingsDiffTests: XCTestCase {
    func testNoChangeMakesAnEmptyPatch() {
        XCTAssertTrue(SettingsDiff.patch(from: .defaults, to: .defaults).isEmpty)
    }

    func testOnlyWhatChangesIsInThePatch() {
        let old = SettingsView.defaults
        var new = old
        new.codeBlocks = "skip"
        new.pitch = 0.4
        let p = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(p.codeBlocks, "skip")
        XCTAssertEqual(p.pitch, 0.4)
        XCTAssertNil(p.speed)
        XCTAssertNil(p.voice)
        XCTAssertNil(p.threads)
    }

    func testEveryScalarFieldIsCovered() {
        var new = SettingsView.defaults
        new.voice = "F1"; new.language = "en"; new.speed = 2; new.pitch = 0.5
        new.volume = 0.5; new.engine = "system"
        new.threads = new.threads == 2 ? 3 : 2; new.sessions = 3; new.prefetch = 9
        new.codeBlocks = "skip"
        new.autoplay = false; new.seekStep = 9; new.lineWidth = 80; new.lineSpacing = 2
        new.readingRuler = false; new.keepVoiceMinutes = 30; new.outputDevice = "Casque"
        let p = SettingsDiff.patch(from: .defaults, to: new)
        XCTAssertEqual(p.voice, "F1"); XCTAssertEqual(p.language, "en"); XCTAssertEqual(p.speed, 2)
        XCTAssertEqual(p.pitch, 0.5); XCTAssertEqual(p.volume, 0.5); XCTAssertEqual(p.engine, "system")
        XCTAssertEqual(p.threads, new.threads); XCTAssertEqual(p.sessions, 3)
        XCTAssertEqual(p.prefetch, 9); XCTAssertEqual(p.codeBlocks, "skip"); XCTAssertEqual(p.autoplay, false)
        XCTAssertEqual(p.seekStep, 9); XCTAssertEqual(p.lineWidth, 80); XCTAssertEqual(p.lineSpacing, 2)
        XCTAssertEqual(p.readingRuler, false); XCTAssertEqual(p.keepVoiceMinutes, 30)
        XCTAssertEqual(p.outputDevice, "Casque")
    }

    func testAnUnlistedValueStaysInTheMenu() {
        let options: [(title: String, value: String)] = [("Annoncer", "announce"), ("Sauter", "skip")]
        let kept = Choices.including(options, current: "lire") { "\($0) (écrit à la main)" }
        XCTAssertEqual(kept.map(\.value), ["announce", "skip", "lire"])
        XCTAssertEqual(kept.last?.title, "lire (écrit à la main)")
        XCTAssertEqual(Choices.including(options, current: "skip") { $0 }.map(\.value), ["announce", "skip"])
    }

    func testAVoicePerLanguageBecomesItsOwnPatch() {
        var old = SettingsView.defaults
        old.voicesByLanguage = []
        var new = old
        new.voicesByLanguage = [LanguageVoice(language: "fr", voice: "ff_siwis")]
        let patch = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(patch.voiceForLanguage, LanguageVoice(language: "fr", voice: "ff_siwis"))
        XCTAssertNil(patch.voice)
        XCTAssertTrue(SettingsDiff.patch(from: new, to: new).isEmpty)
    }

    /// Deux langues changées dans la même pause : un patch par langue, le
    /// premier portant aussi les autres champs.
    func testTwoChangedLanguagesMakeTwoPatches() {
        var old = SettingsView.defaults
        old.voicesByLanguage = [LanguageVoice(language: "fr", voice: "ff_siwis"),
                                LanguageVoice(language: "en", voice: "am_fenrir")]
        var new = old
        new.speed = 1.2
        new.voicesByLanguage = [LanguageVoice(language: "fr", voice: "p-fenrir60"),
                                LanguageVoice(language: "en", voice: "af_sky")]
        let patches = SettingsDiff.patches(from: old, to: new)
        XCTAssertEqual(patches.count, 2)
        XCTAssertEqual(patches.first?.speed, 1.2)
        XCTAssertEqual(Set(patches.compactMap(\.voiceForLanguage)),
                       [LanguageVoice(language: "fr", voice: "p-fenrir60"),
                        LanguageVoice(language: "en", voice: "af_sky")])
        XCTAssertNil(patches.last?.speed, "le second patch ne redit que sa langue")
        XCTAssertTrue(SettingsDiff.patches(from: new, to: new).isEmpty)
    }

    func testFontAndSizeAreSentAlone() {
        let old = SettingsView.defaults
        var new = old
        new.readingFont = "openDyslexic"
        new.readingSize = 26
        let patch = SettingsDiff.patch(from: old, to: new)
        XCTAssertEqual(patch.readingFont, "openDyslexic")
        XCTAssertEqual(patch.readingSize, 26)
        XCTAssertNil(patch.lineSpacing)
    }
}

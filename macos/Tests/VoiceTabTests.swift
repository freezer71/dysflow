import XCTest
@testable import Sillon

final class VoiceTabTests: XCTestCase {
    func testWithoutAnActiveModelTheSystemVoiceIsListedFirst() {
        let none = VoiceTab.modelOptions([.sample(id: "a", name: "A")])
        XCTAssertEqual(none.map(\.value), ["", "a"])
        XCTAssertEqual(none.first?.title, "Voix du système")
        let active = VoiceTab.modelOptions([.sample(id: "a", name: "A", active: true)])
        XCTAssertEqual(active.map(\.value), ["a"])
    }

    func testLanguagesAreThoseOfTheModelPlusTheOneInPlace() {
        let options = VoiceTab.languageOptions(["fr", "en"], current: "ja")
        XCTAssertEqual(options.map(\.value), ["fr", "en", "ja"])
        XCTAssertEqual(options[0].title, "Français")
        XCTAssertEqual(options[1].title, "Anglais")
        XCTAssertTrue(options[2].title.hasSuffix("(hors liste)"))
        XCTAssertEqual(VoiceTab.languageOptions(["fr"], current: "fr").count, 1)
    }

    func testTheVoiceStateLine() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading), "Voix en chargement…")
        XCTAssertNil(VoiceTab.voiceLine(.ready))
        XCTAssertNil(VoiceTab.voiceLine(.resting))
    }

    /// 1c : le repli se dit une seule fois, avec sa raison.
    func testTheFallbackIsSaidOnceWithItsReason() {
        XCTAssertEqual(VoiceTab.voiceLine(.fallback(reason: "fichier manquant")),
                       "Voix du système en repli : fichier manquant")
    }

    func testThePreviewLineSaysWhyFirst() {
        let idle = PreviewView(pending: false, playing: false, error: nil)
        XCTAssertNil(VoiceTab.previewLine(idle, refusal: nil))
        XCTAssertEqual(VoiceTab.previewLine(idle, refusal: "sortie audio indisponible"),
                       "sortie audio indisponible")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: true, playing: false, error: nil), refusal: nil),
                       "Synthèse de l'aperçu…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: true, error: nil), refusal: nil),
                       "Écoute…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: false, error: "échec"), refusal: nil),
                       "échec")
    }

    func testAVoiceOutsideTheListIsShownAsIs() {
        let voices = Capabilities.supertonic.voices
        XCTAssertNil(VoiceTab.unlistedVoice("", voices))
        XCTAssertNil(VoiceTab.unlistedVoice("M1", voices))
        XCTAssertEqual(VoiceTab.unlistedVoice("Aria", voices), "Aria")
    }

    func testAClonedVoiceIsAskedForOnlyWhenNeeded() {
        XCTAssertFalse(VoiceTab.needsReference(Capabilities.supertonic))
        let cloning = ModelCapabilities(family: "chatterbox", voices: [], languages: ["en"],
                                        honoured: [], requiresReference: false)
        XCTAssertTrue(VoiceTab.needsReference(cloning), "aucune voix intégrée")
        let system = ModelCapabilities(family: "", voices: [], languages: [], honoured: [],
                                       requiresReference: false)
        XCTAssertFalse(VoiceTab.needsReference(system), "voix du système")
    }

    func testNumbersAreWrittenTheFrenchWay() {
        XCTAssertEqual(SettingFormat.decimal(1.25), "1,25")
        XCTAssertEqual(SettingFormat.decimal(1), "1")
        XCTAssertEqual(SettingFormat.percent(0.6), "60 %")
        XCTAssertEqual(LanguageName.display("de"), "Allemand")
    }
}

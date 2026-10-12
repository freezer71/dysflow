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

    func testLanguagesStartWithAutomaticThenTheModelsOnesPlusTheOneInPlace() {
        let options = VoiceTab.languageOptions(["fr", "en"], current: "ja")
        XCTAssertEqual(options.map(\.value), ["auto", "fr", "en", "ja"])
        XCTAssertEqual(options[0].title, "Automatique")
        XCTAssertEqual(options[1].title, "Français")
        XCTAssertTrue(options[3].title.hasSuffix("(hors liste)"))
        XCTAssertEqual(VoiceTab.languageOptions(["fr"], current: "fr").count, 2)
    }

    func testKokoroOffersOneVoicePerLanguage() {
        let caps = Capabilities.kokoro
        XCTAssertTrue(VoiceTab.followsLanguage(caps))
        XCTAssertFalse(VoiceTab.followsLanguage(Capabilities.supertonic))
        XCTAssertEqual(VoiceTab.voices(caps, language: "fr").map(\.id), ["p-fenrir60", "ff_siwis"])
        XCTAssertEqual(VoiceTab.voices(caps, language: "en").map(\.id), ["af_bella", "af_sky"])
        var values = SettingsView.defaults
        values.voicesByLanguage = []
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_bella", "rien de choisi : la première")
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "af_sky")]
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_sky")
    }

    func testTheDocumentLanguageSaysWhatWasDetected() {
        let detected = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.title),
                       ["Automatique (français)", "Français", "Anglais"])
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.value), ["", "fr", "en"])
        let unknown = DocumentLanguageView(detected: nil, chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(unknown).first?.title, "Automatique (non détectée)")
    }

    func testAMixedDocumentIsSaidWithItsMajority() {
        let mixed = DocumentLanguageView(detected: "fr", chosen: "en", effective: "en", minorityShare: 0.38, mixed: true)
        XCTAssertEqual(VoiceTab.mixedLine(mixed), "Document mêlé : 62 % en français")
        let plain = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0.1, mixed: false)
        XCTAssertNil(VoiceTab.mixedLine(plain))
    }

    func testTheFirstPreparationIsSaid() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: true), "Voix en préparation (première fois)…")
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: false), "Voix en chargement…")
        XCTAssertEqual(VoiceTab.voiceLine(.ready, preparing: true), "Voix en préparation (première fois)…",
                       "la préparation tourne à part du lecteur, voix prête ou au repos")
        XCTAssertEqual(VoiceTab.voiceLine(.resting, preparing: true), "Voix en préparation (première fois)…")
    }

    /// M1 : l'offre en un clic, avec sa taille écrite comme l'onglet Modèles
    /// (`ByteCountFormatter`, base 1000).
    func testTheOfferSaysItsSizeLikeTheModelsTab() {
        let offer = VoiceOfferView(id: "kokoro_82m_mlx", bytes: 342_000_000, former: "F2", message: "")
        let size = ByteCountFormatter.string(fromByteCount: 342_000_000, countStyle: .file)
        XCTAssertTrue(size.hasPrefix("342"), size)
        XCTAssertEqual(VoiceTab.offerTitle(offer), "Passer à la nouvelle voix (\(size))")
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

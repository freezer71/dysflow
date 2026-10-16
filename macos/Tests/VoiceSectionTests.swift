import XCTest
@testable import Sillon

@MainActor
final class VoiceSectionTests: XCTestCase {
    /// Une voix pour tout : celle écrite pour le français vaut pour toutes
    /// les langues.
    func testOneVoiceIsShownForEveryLanguage() {
        var values = SettingsView.defaults
        values.voicesByLanguage = [LanguageVoice(language: "fr", voice: "am_fenrir"),
                                   LanguageVoice(language: "en", voice: "am_fenrir")]
        XCTAssertEqual(VoiceSection.voice(in: values, Capabilities.kokoro), "am_fenrir")
    }

    /// Une voix retirée du modèle (ou écrite à la main) se montre comme celle
    /// que le moteur fait parler : la première du modèle.
    func testAVoiceGoneFromTheModelFallsBackToItsFirst() {
        var values = SettingsView.defaults
        values.voicesByLanguage = [LanguageVoice(language: "fr", voice: "inconnue")]
        XCTAssertEqual(VoiceSection.voice(in: values, Capabilities.kokoro), "p-fenrir60")
    }

    /// La voix de macOS garde la sienne dans `voice`.
    func testTheMacVoiceIsReadFromVoice() {
        let caps = ModelCapabilities(
            family: "system",
            voices: [VoiceChoice(id: "Thomas", label: "Thomas · fr_FR", language: "fr"),
                     VoiceChoice(id: "Samantha", label: "Samantha · en_US", language: "en")],
            languages: ["fr", "en"], honoured: ["voice"])
        var values = SettingsView.defaults
        values.voice = "Samantha"
        XCTAssertEqual(VoiceSection.voice(in: values, caps), "Samantha")
        XCTAssertEqual(VoiceSection.language(of: "Samantha", caps), "en", "l'aperçu dans la langue de la voix")
        XCTAssertEqual(VoiceSection.language(of: "inconnue", caps), "fr")
    }

    func testSizesAreWrittenTheFrenchWayInBase1000() {
        XCTAssertEqual(SizeFormat.bytes(342_264_783), "342 Mo")
        XCTAssertEqual(SizeFormat.bytes(92_361_055), "92,4 Mo")
        XCTAssertEqual(SizeFormat.bytes(522_320), "522 Ko")
        XCTAssertEqual(SizeFormat.bytes(1_500_000_000), "1,5 Go")
        XCTAssertEqual(SizeFormat.bytes(5_000_000), "5 Mo")
        XCTAssertEqual(SizeFormat.bytes(999_700), "1 Mo", "jamais « 1 000 Ko »")
        XCTAssertEqual(SizeFormat.bytes(999), "999 octets")
        XCTAssertEqual(SizeFormat.bytes(1), "1 octet")
        XCTAssertEqual(SizeFormat.bytes(0), "0 octet")
        XCTAssertEqual(SizeFormat.bytes(999_499), "999 Ko", "sous la frontière")
        XCTAssertEqual(SizeFormat.bytes(999_500), "1 Mo", "à la frontière : jamais « 1 000 Ko »")
    }

    /// L'avis et l'offre se disent une fois par valeur, pas à chaque retour
    /// sur l'onglet.
    @MainActor
    func testAnAnnouncementIsMadeOncePerValue() {
        var once = AnnounceOnce()
        XCTAssertTrue(once.shouldSay("Nouvelle voix"))
        XCTAssertFalse(once.shouldSay("Nouvelle voix"))
        XCTAssertTrue(once.shouldSay("Autre avis"))
    }

    /// Les tests d'interface (compilés seulement) atteignent le bandeau et
    /// les voix de Kokoro sous le lecteur muet.
    func testUITestArgumentsStageTheUnreadableConfigAndKokoro() {
        XCTAssertNil(UITestStaging.staging(for: ["--ui-testing"]))
        var s = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing", "--ui-testing-config-illisible"])?(&s)
        XCTAssertTrue(s.configUnreadable)
        XCTAssertNil(UITestStaging.capabilities(for: ["--ui-testing"]))
        let kokoro = UITestStaging.capabilities(for: ["--ui-testing", "--ui-testing-kokoro"])
        XCTAssertEqual(kokoro?.family, "kokoro")
        XCTAssertEqual(kokoro?.voices.map(\.id), Capabilities.kokoro.voices.map(\.id))
    }

    func testModelsAreSuggestedForAFallbackOrAnUnavailableVoice() {
        XCTAssertTrue(VoiceSection.suggestsModels(.fallback(reason: "x")))
        XCTAssertTrue(VoiceSection.suggestsModels(.unavailable(reason: "x")))
        XCTAssertFalse(VoiceSection.suggestsModels(.ready))
        XCTAssertFalse(VoiceSection.suggestsModels(.loading))
    }

    func testThePreviewLineSaysWhyFirst() {
        let idle = PreviewView(pending: false, playing: false, error: nil)
        XCTAssertNil(VoiceSection.previewLine(idle, refusal: nil))
        XCTAssertEqual(VoiceSection.previewLine(idle, refusal: "sortie audio indisponible"),
                       "sortie audio indisponible")
        XCTAssertEqual(VoiceSection.previewLine(PreviewView(pending: true, playing: false, error: nil), refusal: nil),
                       "Synthèse de l'aperçu…")
        XCTAssertEqual(VoiceSection.previewLine(PreviewView(pending: false, playing: true, error: nil), refusal: nil),
                       "Écoute…")
        XCTAssertEqual(VoiceSection.previewLine(PreviewView(pending: false, playing: false, error: "échec"), refusal: nil),
                       "échec")
    }

    func testNumbersAreWrittenTheFrenchWay() {
        XCTAssertEqual(SettingFormat.decimal(1.25), "1,25")
        XCTAssertEqual(SettingFormat.decimal(1), "1")
        XCTAssertEqual(SettingFormat.percent(0.6), "60 %")
    }
}

import XCTest
@testable import Sillon

@MainActor
final class VoiceSectionTests: XCTestCase {
    func testKokoroOffersOneVoicePerLanguage() {
        let caps = Capabilities.kokoro
        XCTAssertTrue(VoiceSection.followsLanguage(caps))
        XCTAssertFalse(VoiceSection.followsLanguage(Capabilities.system))
        XCTAssertEqual(VoiceSection.voices(caps, language: "fr").map(\.id), ["p-fenrir60"])
        XCTAssertEqual(VoiceSection.voices(caps, language: "en").map(\.id), ["af_bella", "am_fenrir", "af_heart"])
        var values = SettingsView.defaults
        values.voicesByLanguage = []
        XCTAssertEqual(VoiceSection.voice(for: "en", in: values, caps), "af_bella", "rien de choisi : la première")
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "am_fenrir")]
        XCTAssertEqual(VoiceSection.voice(for: "en", in: values, caps), "am_fenrir")
    }

    /// Kokoro configuré mais en repli (voix du système) ou indisponible : ni
    /// ses voix ni ses « Écouter ». Prêt, au repos, en chargement : oui.
    func testKokoroVoicesShowOnlyWhenKokoroReallySpeaks() {
        let kokoro = Capabilities.kokoro
        for state in [VoiceState.ready, .resting, .loading] {
            XCTAssertTrue(VoiceSection.showsKokoroVoices(kokoro, voice: state), "\(state)")
        }
        XCTAssertFalse(VoiceSection.showsKokoroVoices(kokoro, voice: .fallback(reason: "Metal indisponible")))
        XCTAssertFalse(VoiceSection.showsKokoroVoices(kokoro, voice: .unavailable(reason: "aucune session")))
        XCTAssertFalse(VoiceSection.showsKokoroVoices(Capabilities.system, voice: .ready), "pas Kokoro")
    }

    /// Une voix retirée du catalogue (ou écrite à la main) se montre comme
    /// celle que le moteur fait parler : la recommandée de sa langue. Le menu
    /// ne liste que les voix proposées, sans « (hors liste) ».
    func testARetiredVoiceShowsAsTheRecommendedOne() {
        let caps = Capabilities.kokoro
        var values = SettingsView.defaults
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "bf_emma"),
                                   LanguageVoice(language: "fr", voice: "ff_siwis")]
        XCTAssertEqual(VoiceSection.voice(for: "en", in: values, caps), "af_bella")
        XCTAssertEqual(VoiceSection.voice(for: "fr", in: values, caps), "p-fenrir60")
        XCTAssertEqual(VoiceSection.voiceOptions(caps, language: "en").map(\.value), ["af_bella", "am_fenrir", "af_heart"])
        XCTAssertEqual(VoiceSection.voiceOptions(caps, language: "fr").map(\.title), ["p-fenrir60 · française, recommandée"])
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
        XCTAssertEqual(LanguageName.display("de"), "Allemand")
    }
}

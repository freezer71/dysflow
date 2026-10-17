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

    private let macVoices = ModelCapabilities(
        family: "system",
        voices: [VoiceChoice(id: "Thomas", label: "Thomas · fr_FR", language: "fr"),
                 VoiceChoice(id: "Samantha", label: "Samantha · en_US", language: "en")],
        languages: ["fr", "en"], honoured: ["voice"])

    /// Aucune voix de macOS choisie : le menu dit la voix par défaut (celle
    /// que `say` prend), pas la première de la liste ; l'aperçu en français.
    func testNoMacVoiceChosenShowsTheMacDefault() {
        var values = SettingsView.defaults
        values.voice = ""
        XCTAssertEqual(VoiceSection.voice(in: values, macVoices), "")
        XCTAssertEqual(VoiceSection.language(of: "", macVoices), "fr")
        XCTAssertEqual(VoiceSection.voiceOptions(macVoices, quality: [:]).first?.title, "Voix par défaut de macOS")
        XCTAssertEqual(VoiceSection.voiceOptions(macVoices, quality: [:]).map(\.value), ["", "Thomas", "Samantha"])
    }

    func testAMacVoiceNotListedShowsTheMacDefault() {
        var values = SettingsView.defaults
        values.voice = "Inconnue"
        XCTAssertEqual(VoiceSection.voice(in: values, macVoices), "")
    }

    /// Kokoro n'a pas de voix par défaut à part : ses seules voix.
    func testKokoroOptionsAreItsVoicesOnly() {
        XCTAssertEqual(VoiceSection.voiceOptions(Capabilities.kokoro).map(\.value),
                       Capabilities.kokoro.voices.map(\.id))
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

    func testPremiumMacVoicesComeFirst() {
        let ordered = VoiceSection.ordered(["Thomas", "Amélie (Premium)", "Audrey (Améliorée)"],
                                           quality: ["Amélie (Premium)": 3, "Audrey (Améliorée)": 2])
        XCTAssertEqual(ordered, ["Amélie (Premium)", "Audrey (Améliorée)", "Thomas"])
    }

    /// Les voix Premium passent en tête, après la voix par défaut de macOS.
    func testTheMacDefaultStaysFirstBeforePremiumVoices() {
        let options = VoiceSection.voiceOptions(macVoices, quality: ["Samantha": 3])
        XCTAssertEqual(options.map(\.value), ["", "Samantha", "Thomas"])
    }

    private typealias Mac = VoiceSection.MacVoice

    /// « Zoe » et « Zoe (Premium) » côte à côte : seule la seconde monte.
    func testOnlyTheMarkedTwinIsPromoted() {
        let q = VoiceSection.quality(of: ["Zoe", "Zoe (Premium)"],
                                     voices: [Mac(name: "Zoe", quality: 1), Mac(name: "Zoe", quality: 3)])
        XCTAssertEqual(q["Zoe (Premium)"], 3)
        XCTAssertEqual(q["Zoe"], 1, "la voix compacte reste à sa place")
    }

    /// `say` en français accorde : « Thomas (Amélioré) ».
    func testAMasculineEnhancedVoiceIsMatched() {
        let q = VoiceSection.quality(of: ["Thomas (Amélioré)", "Audrey (Améliorée)"],
                                     voices: [Mac(name: "Thomas", quality: 2), Mac(name: "Audrey", quality: 2)])
        XCTAssertEqual(q["Thomas (Amélioré)"], 2)
        XCTAssertEqual(q["Audrey (Améliorée)"], 2)
    }

    func testAnEnglishEnhancedVoiceIsMatchedWhateverTheCase() {
        let q = VoiceSection.quality(of: ["Ava (enhanced)"], voices: [Mac(name: "Ava (Enhanced)", quality: 2)])
        XCTAssertEqual(q["Ava (enhanced)"], 2)
    }

    /// Sans voix AVFoundation du même nom, ni la parenthèse ni rien d'autre
    /// ne fait monter une voix.
    func testWithoutAMatchingVoiceTheQualityIsOne() {
        let q = VoiceSection.quality(of: ["Zoe (Premium)", "Thomas"], voices: [Mac(name: "Amélie", quality: 3)])
        XCTAssertEqual(q["Zoe (Premium)"], 1)
        XCTAssertEqual(q["Thomas"], 1)
    }

    /// Une voix Premium que `say` nomme sans parenthèse monte si elle n'a pas
    /// de jumeau marqué.
    func testABareNameWithoutTwinTakesItsQuality() {
        let q = VoiceSection.quality(of: ["Amélie"], voices: [Mac(name: "Amélie", quality: 3)])
        XCTAssertEqual(q["Amélie"], 3)
    }
}

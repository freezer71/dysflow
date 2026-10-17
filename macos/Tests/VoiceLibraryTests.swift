import XCTest
@testable import Sillon

/// Un moteur de bibliothèque pour les tests : il note ce qu'on lui demande.
@MainActor
final class FakeVoiceLibraryBackend: VoiceLibraryBackend {
    var voices: [LibraryVoice] = []
    private(set) var log: [String] = []
    var failPreview: String?

    func loadVoiceLibrary() async -> [LibraryVoice] { voices }
    func previewLibraryVoice(model: String, voice: String, text: String, speed: Float) async throws {
        log.append("écoute \(model)/\(voice) « \(text) » \(speed)")
        if let failPreview { throw SillonError.Failed(message: failPreview) }
    }
    func stopPreview() async { log.append("arrêt") }
    func releaseVoicePreview() async { log.append("relâche") }
}

@MainActor
final class VoiceLibraryTests: XCTestCase {
    private func kokoro(_ id: String, accent: String, gender: String, recommended: Bool = false,
                        modelActive: Bool = true, active: Bool = false) -> LibraryVoice {
        LibraryVoice(modelId: "kokoro_82m_mlx", modelLabel: "Kokoro", voiceId: id, label: id,
                     language: accent == "française" ? "fr" : "en", accent: accent, gender: gender,
                     locale: "", recommended: recommended, modelActive: modelActive, active: active)
    }

    private func mac(_ name: String, locale: String, accent: String, modelActive: Bool = false,
                     active: Bool = false) -> LibraryVoice {
        LibraryVoice(modelId: "system", modelLabel: "macOS", voiceId: name, label: name,
                     language: String(locale.prefix(2)), accent: accent, gender: "", locale: locale,
                     recommended: false, modelActive: modelActive, active: active)
    }

    private var sample: [LibraryVoice] {
        [mac("Thomas", locale: "fr_FR", accent: "française"),
         kokoro("am_fenrir", accent: "américaine", gender: "homme", active: true),
         mac("Zoe (Premium)", locale: "en_US", accent: "américaine"),
         kokoro("af_bella", accent: "américaine", gender: "femme", recommended: true),
         kokoro("p-fenrir60", accent: "française", gender: "femme", recommended: true),
         kokoro("bf_emma", accent: "britannique", gender: "femme")]
    }

    private let macVoices = [VoiceSection.MacVoice(name: "Thomas", quality: 1, gender: "homme"),
                             VoiceSection.MacVoice(name: "Zoe (Premium)", quality: 3, gender: "femme"),
                             VoiceSection.MacVoice(name: "Zoe", quality: 1, gender: "femme")]

    func testMacVoicesTakeTheirGenderAndQualityFromAVFoundation() {
        let entries = VoiceLibrary.entries(sample, macVoices: macVoices)
        let zoe = entries.first { $0.voiceID == "Zoe (Premium)" }!
        XCTAssertEqual(zoe.gender, "femme")
        XCTAssertEqual(zoe.quality, 3)
        XCTAssertEqual(zoe.qualityBadge, "Premium")
        let thomas = entries.first { $0.voiceID == "Thomas" }!
        XCTAssertEqual(thomas.gender, "homme")
        XCTAssertNil(thomas.qualityBadge)
        let bella = entries.first { $0.voiceID == "af_bella" }!
        XCTAssertNil(bella.qualityBadge, "Kokoro n'a pas de qualité de macOS")
        XCTAssertEqual(bella.gender, "femme", "Kokoro : le genre du catalogue")
    }

    func testTheActiveModelComesFirstThenRecommendedThenAlphabetical() {
        let ordered = VoiceLibrary.ordered(VoiceLibrary.entries(sample, macVoices: macVoices))
        XCTAssertEqual(ordered.map(\.voiceID),
                       ["af_bella", "p-fenrir60", "am_fenrir", "bf_emma", "Thomas", "Zoe (Premium)"])
    }

    func testFiltersCombineAndSearchIgnoresCaseAndAccents() {
        let entries = VoiceLibrary.entries(sample, macVoices: macVoices)
        func ids(_ f: VoiceLibrary.Filter) -> [String] { VoiceLibrary.filtered(entries, f).map(\.voiceID) }
        XCTAssertEqual(ids(.init(model: "system")), ["Thomas", "Zoe (Premium)"])
        XCTAssertEqual(ids(.init(accent: "française")), ["p-fenrir60", "Thomas"])
        XCTAssertEqual(ids(.init(gender: "homme")), ["am_fenrir", "Thomas"])
        XCTAssertEqual(ids(.init(model: "kokoro_82m_mlx", gender: "femme")), ["af_bella", "p-fenrir60", "bf_emma"])
        XCTAssertEqual(ids(.init(search: "FENRIR")), ["p-fenrir60", "am_fenrir"])
        XCTAssertEqual(ids(.init(search: "zoé")), ["Zoe (Premium)"], "sans accents")
        XCTAssertEqual(ids(.init(search: "  ")).count, 6, "une recherche vide garde tout")
    }

    func testTheFilterOptionsComeFromTheVoices() {
        let entries = VoiceLibrary.entries(sample, macVoices: macVoices)
        XCTAssertEqual(VoiceLibrary.modelOptions(entries).map(\.title), ["Tous", "Kokoro", "macOS"])
        XCTAssertEqual(VoiceLibrary.modelOptions(entries).map(\.value), ["", "kokoro_82m_mlx", "system"])
        XCTAssertEqual(VoiceLibrary.accentOptions(entries).map(\.title),
                       ["Tous", "américaine", "britannique", "française"])
        XCTAssertEqual(VoiceLibrary.genderOptions.map(\.title), ["Tous", "Femme", "Homme"])
    }

    func testTheRowDescribesTheVoiceInFullForVoiceOver() {
        let entries = VoiceLibrary.entries(sample, macVoices: macVoices)
        let zoe = entries.first { $0.voiceID == "Zoe (Premium)" }!
        XCTAssertEqual(zoe.details, "macOS · américaine · femme")
        let fenrir = entries.first { $0.voiceID == "am_fenrir" }!
        XCTAssertEqual(VoiceLibrary.accessibilityLabel(fenrir),
                       "am_fenrir, Kokoro, américaine, homme, voix utilisée")
        XCTAssertEqual(VoiceLibrary.accessibilityLabel(zoe), "Zoe (Premium), macOS, américaine, femme, Premium")
    }

    // MARK: - Le magasin

    func testItOpensOnTheVoiceInUseWithTheSampleTextAndTheReadingSpeed() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        let store = VoiceLibraryStore(backend: backend, sampleText: "Bonjour, voici.", speed: 1.5,
                                      macVoices: { self.macVoices })
        await store.load()
        XCTAssertEqual(store.selection, "kokoro_82m_mlx/am_fenrir")
        XCTAssertEqual(store.text, "Bonjour, voici.")
        XCTAssertEqual(store.speed, 1.5)
        XCTAssertEqual(store.visible.count, 6)
        store.filter.gender = "homme"
        XCTAssertEqual(store.visible.map(\.voiceID), ["am_fenrir", "Thomas"])
    }

    func testPlayingSendsTheTextAndSpeedThenStopsOnASecondPress() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        let store = VoiceLibraryStore(backend: backend, sampleText: "Salut.", speed: 1, macVoices: { [] })
        await store.load()
        store.text = "Un texte à moi."
        store.speed = 2
        let bella = store.visible.first { $0.voiceID == "af_bella" }!
        await store.toggle(bella)
        XCTAssertEqual(store.playing, bella.id)
        XCTAssertEqual(backend.log, ["écoute kokoro_82m_mlx/af_bella « Un texte à moi. » 2.0"])
        await store.toggle(bella)
        XCTAssertNil(store.playing)
        XCTAssertEqual(backend.log.last, "arrêt")
    }

    func testThePlayingMarkClearsWhenTheEngineGoesQuiet() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        let store = VoiceLibraryStore(backend: backend, sampleText: "Salut.", speed: 1, macVoices: { [] })
        await store.load()
        await store.toggle(store.visible[0])
        store.previewChanged(active: true)
        store.previewChanged(active: false)
        XCTAssertNil(store.playing, "fini : le bouton redevient « Écouter »")
    }

    /// B demandée pendant que A sonne : le moteur reste occupé d'un trait
    /// (aucun « faux » entre les deux) ; le repos suivant finit B.
    func testPlayingBWhileAIsStillSoundingEndsWithNothingMarked() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        let store = VoiceLibraryStore(backend: backend, sampleText: "Salut.", speed: 1, macVoices: { [] })
        await store.load()
        let (a, b) = (store.visible[0], store.visible[1])
        await store.toggle(a)
        store.previewChanged(active: true)
        await store.toggle(b)
        XCTAssertEqual(store.playing, b.id)
        store.previewChanged(active: false)
        XCTAssertNil(store.playing)
    }

    /// Aucune voix cochée : rien n'est choisi d'avance, « Utiliser » ne
    /// changerait rien en silence.
    func testNothingIsPreselectedWhenNoRowIsInUse() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample.map { v in
            var v = v
            v.active = false
            return v
        }
        let store = VoiceLibraryStore(backend: backend, sampleText: "", speed: 1, macVoices: { [] })
        await store.load()
        XCTAssertNil(store.selection)
        XCTAssertNil(store.selected)
    }

    /// La voix par défaut de macOS a sa ligne, cochée quand elle lit.
    func testTheDefaultMacVoiceIsPreselectedWhenItReads() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample.map { v in
            var v = v
            v.active = false
            v.modelActive = v.modelId == "system"
            return v
        } + [LibraryVoice(modelId: "system", modelLabel: "macOS", voiceId: "", label: "Voix par défaut de macOS",
                          language: "fr", accent: "française", gender: "", locale: "fr_FR", recommended: false,
                          modelActive: true, active: true)]
        let store = VoiceLibraryStore(backend: backend, sampleText: "", speed: 1, macVoices: { [] })
        await store.load()
        XCTAssertEqual(store.selection, "system/")
        XCTAssertEqual(store.selected?.label, "Voix par défaut de macOS")
        XCTAssertNil(store.selected?.qualityBadge)
    }

    /// Une ligne choisie que les filtres cachent n'est ni écoutée ni utilisée.
    func testAHiddenSelectionIsNotActedOn() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        let store = VoiceLibraryStore(backend: backend, sampleText: "", speed: 1, macVoices: { [] })
        await store.load()
        XCTAssertEqual(store.selected?.voiceID, "am_fenrir")
        store.filter.model = "system"
        XCTAssertNil(store.selected)
        store.filter.model = ""
        XCTAssertEqual(store.selected?.voiceID, "am_fenrir")
    }

    /// La feuille ne montre que l'échec de son propre aperçu.
    func testTheSheetShowsOnlyTheErrorOfItsOwnPreview() {
        let failed = PreviewView(pending: false, playing: false, error: "vieil échec", preparing: false)
        XCTAssertNil(VoiceLibrarySheet.status(failed, refusal: nil, requested: false))
        XCTAssertEqual(VoiceLibrarySheet.status(failed, refusal: nil, requested: true), "vieil échec")
        let preparing = PreviewView(pending: true, playing: false, error: nil, preparing: true)
        XCTAssertEqual(VoiceLibrarySheet.status(preparing, refusal: nil, requested: true), "Préparation de la voix…")
    }

    func testTheTextLimitIsShown() {
        XCTAssertEqual(VoiceLibraryStore.countLine("Bonjour"), "7 caractères sur 500 au plus")
        XCTAssertEqual(VoiceLibraryStore.countLine(String(repeating: "a", count: 600)),
                       "600 caractères : seuls les 500 premiers seront lus")
    }

    func testARefusedPreviewIsShownAndNothingPlays() async {
        let backend = FakeVoiceLibraryBackend()
        backend.voices = sample
        backend.failPreview = "Rien à lire"
        var said: [String] = []
        let store = VoiceLibraryStore(backend: backend, sampleText: "", speed: 1, macVoices: { [] },
                                      announce: { said.append($0) })
        await store.load()
        await store.toggle(store.visible[0])
        XCTAssertNil(store.playing)
        XCTAssertEqual(store.error, "Rien à lire")
        XCTAssertEqual(said, ["Rien à lire"])
    }

    func testClosingStopsAndReleasesThePreviewModel() async {
        let backend = FakeVoiceLibraryBackend()
        let store = VoiceLibraryStore(backend: backend, sampleText: "", speed: 1, macVoices: { [] })
        await store.close()
        XCTAssertEqual(backend.log, ["arrêt", "relâche"])
    }

    func testThePreviewSpeedStaysWithinTheReaderBounds() {
        let store = VoiceLibraryStore(backend: FakeVoiceLibraryBackend(), sampleText: "", speed: 7,
                                      macVoices: { [] })
        XCTAssertEqual(store.speed, 3)
    }
}

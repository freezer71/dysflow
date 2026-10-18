import XCTest
@testable import Sillon

/// L'essai du micro, de bout en bout, avec un faux moteur : rien n'est saisi,
/// le modèle de la dictée ne change pas, les résultats restent.
@MainActor
final class MicTrialControllerTests: XCTestCase {
    private var engine: FakeDictationEngine!
    private var defaults: UserDefaults!
    private let suite = "MicTrialControllerTests"
    private var now = 0.0
    private var dictating = false
    private var compute = 0.0
    private var made: [String] = []
    private var released: [String] = []

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
        engine = FakeDictationEngine()
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: suite)
    }

    private func makeTrial(mlx: Bool = true) -> MicTrialController {
        MicTrialController(
            make: { [unowned self] id in
                made.append(id)
                guard mlx else { return TrialEngine(engine: engine) }
                return TrialEngine(engine: engine,
                                   load: { TrialMemory(bytes: 800 << 20, measured: true) },
                                   computeSeconds: { [unowned self] in compute },
                                   release: { [unowned self] in released.append(id) })
            },
            store: TrialResultsStore(defaults: defaults),
            language: { .french },
            dictationActive: { [unowned self] in dictating },
            clock: { [unowned self] in now })
    }

    private func loud() -> [[Float]] {
        var column = [Float](repeating: 0, count: 12)
        column[2] = 0.9
        return Array(repeating: column, count: 4)
    }

    /// `seconds` de silence : des tranches sous le seuil, 40 par seconde.
    private func quiet(_ seconds: Double) -> [[Float]] {
        Array(repeating: [Float](repeating: 0.05, count: 12), count: Int(seconds * 40))
    }

    func testTheTrialStopsByItselfOnceTheSentenceIsSaid() async {
        let trial = makeTrial()
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.send(.spectrum(loud()))
        let sentence = trial.sentence
        engine.finalText = sentence
        engine.send(.text(String(sentence.dropLast(8)), isFinal: false))
        XCTAssertEqual(trial.phase, .listening, "une phrase pas finie ne l'arrête pas")
        // Sans ponctuation ni majuscules : les mots suffisent.
        engine.send(.text(sentence.lowercased().replacingOccurrences(of: ".", with: ""), isFinal: false))
        let done = await eventuallyTrue { trial.phase == .idle }
        XCTAssertTrue(done)
        XCTAssertEqual(engine.finished, 1)
        XCTAssertEqual(trial.results["parakeet_tdt_v3"]?.precision, 100)
    }

    func testTheTrialStopsByItselfAfterASilenceThatFollowsSpeech() async {
        let trial = makeTrial()
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.send(.spectrum(quiet(3)))
        XCTAssertEqual(trial.phase, .listening, "un silence avant de parler ne compte pas")
        engine.send(.spectrum(loud()))
        engine.send(.spectrum(quiet(2)))
        XCTAssertEqual(trial.phase, .listening, "pas de texte encore : on attend")
        engine.send(.text("le petit", isFinal: false))
        engine.send(.spectrum(loud()))
        engine.send(.spectrum(quiet(1)))
        XCTAssertEqual(trial.phase, .listening, "une pause d'une seconde n'arrête rien")
        engine.finalText = "le petit"
        engine.send(.spectrum(quiet(0.6)))
        let done = await eventuallyTrue { trial.phase == .idle }
        XCTAssertTrue(done, "1,6 s de silence après la parole : l'essai s'arrête")
        XCTAssertEqual(engine.finished, 1)
    }

    func testFreeTextStopsOnlyOnSilence() async {
        let trial = makeTrial()
        trial.freeText = true
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.send(.spectrum(loud()))
        engine.send(.text(trial.sentence, isFinal: false))
        XCTAssertEqual(trial.phase, .listening, "texte libre : pas de phrase à finir")
        engine.finalText = trial.sentence
        engine.send(.spectrum(quiet(1.6)))
        let done = await eventuallyTrue { trial.phase == .idle }
        XCTAssertTrue(done)
    }

    func testAFullTrialScoresTheSentenceAndKeepsTheResult() async {
        let trial = makeTrial()
        var paused = false
        trial.willStart = { paused = true }
        let sentence = trial.sentence
        now = 1
        trial.start(model: "parakeet_tdt_v3")
        XCTAssertTrue(paused, "la lecture se met en pause, comme pour la dictée")
        XCTAssertTrue(trial.isRunning)
        let listening = await eventuallyTrue { trial.phase == .listening }
        XCTAssertTrue(listening)
        now = 2
        engine.send(.spectrum(loud()))
        XCTAssertTrue(trial.speechHeard)
        now = 2.6
        engine.send(.text("Le petit chat", isFinal: false))
        XCTAssertEqual(trial.transcript.text, "Le petit chat")
        now = 5
        compute = 1.6
        engine.finalText = sentence
        trial.stop()
        let done = await eventuallyTrue { trial.phase == .idle }
        XCTAssertTrue(done)
        let result = trial.results["parakeet_tdt_v3"]
        XCTAssertEqual(result?.precision, 100)
        XCTAssertEqual(result?.firstDelay ?? -1, 0.6, accuracy: 1e-9)
        XCTAssertEqual(result?.firstDelayFromSpeech, true)
        XCTAssertEqual(result?.speed ?? -1, 2.5, accuracy: 1e-9, "4 s d'audio pour 1,6 s de calcul")
        XCTAssertEqual(result?.memoryBytes, 800 << 20)
        XCTAssertEqual(TrialResultsStore(defaults: defaults).all()["parakeet_tdt_v3"], result)
        XCTAssertNotNil(trial.announcement)
        XCTAssertEqual(engine.finished, 1)
    }

    func testTheDictationModelIsNeverChanged() async {
        let before = UserDefaults.standard.string(forKey: DictationModel.defaultsKey)
        let trial = makeTrial()
        trial.start(model: "whisper_small_4bit")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "bonjour"
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        XCTAssertEqual(UserDefaults.standard.string(forKey: DictationModel.defaultsKey), before)
        XCTAssertEqual(made, ["whisper_small_4bit"])
    }

    func testNoTrialDuringADictation() {
        dictating = true
        let trial = makeTrial()
        trial.start(model: "system")
        XCTAssertEqual(trial.phase, .failed(MicTrialController.dictationBusy))
        XCTAssertTrue(made.isEmpty)
        XCTAssertFalse(trial.isRunning)
    }

    func testFreeTextHasNoPrecisionAndMacDictationNoSpeed() async {
        let trial = makeTrial(mlx: false)
        trial.freeText = true
        trial.start(model: "system")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "n'importe quoi"
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        let result = trial.results["system"]
        XCTAssertNotNil(result)
        XCTAssertNil(result?.precision)
        XCTAssertNil(result?.speed)
        XCTAssertNil(result?.memoryBytes)
        XCTAssertEqual(result?.freeText, true)
    }

    func testARefusedMicrophoneSaysTheDictationMessage() async {
        engine.startError = .microphoneDenied
        let trial = makeTrial()
        trial.start(model: "system")
        let failed = await eventuallyTrue { trial.phase == .failed(DictationError.microphoneDenied.message) }
        XCTAssertTrue(failed)
        XCTAssertNil(trial.results["system"])
    }

    func testCancelSavesNothing() async {
        let trial = makeTrial()
        trial.start(model: "system")
        _ = await eventuallyTrue { trial.phase == .listening }
        trial.cancel()
        XCTAssertEqual(trial.phase, .idle)
        let cancelled = await eventuallyTrue { engine.cancelled == 1 }
        XCTAssertTrue(cancelled)
        XCTAssertTrue(trial.results.isEmpty)
    }

    func testTheNextTrialReadsAnotherSentence() async {
        let trial = makeTrial()
        let first = trial.sentence
        trial.start(model: "system")
        XCTAssertEqual(trial.sentence, first, "le premier essai lit la phrase montrée")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = first
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        XCTAssertEqual(trial.sentence, first, "la phrase reste à côté de ses résultats")
        trial.start(model: "system")
        XCTAssertNotEqual(trial.sentence, first)
    }

    func testForgettingAModelClearsItsResult() async {
        let trial = makeTrial()
        trial.start(model: "system")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "bonjour"
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        trial.forget("system")
        XCTAssertNil(trial.results["system"])
        XCTAssertNil(TrialResultsStore(defaults: defaults).all()["system"])
    }

    /// Pas de texte final (le modèle n'a pas répondu) : pas de score, le
    /// résultat d'avant reste.
    func testATrialWithoutFinalTextRecordsNothing() async {
        let trial = makeTrial()
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "bonjour"
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        let previous = trial.results["parakeet_tdt_v3"]
        XCTAssertNotNil(previous)
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.send(.text("le petit", isFinal: false))
        engine.finalText = nil
        trial.stop()
        let failed = await eventuallyTrue {
            if case .failed(let message) = trial.phase { return message.hasPrefix("L'essai n'a pas abouti : ") }
            return false
        }
        XCTAssertTrue(failed)
        XCTAssertEqual(trial.results["parakeet_tdt_v3"], previous)
        XCTAssertEqual(TrialResultsStore(defaults: defaults).all()["parakeet_tdt_v3"], previous)
    }

    /// Le modèle essayé est libéré à la fin de l'essai, quelle qu'elle soit.
    func testTheTestedModelIsReleasedAtTheEnd() async {
        let trial = makeTrial()
        trial.start(model: "whisper_small_4bit")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "bonjour"
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        XCTAssertEqual(released, ["whisper_small_4bit"])
        trial.start(model: "whisper_small_4bit")
        _ = await eventuallyTrue { trial.phase == .listening }
        trial.cancel()
        let twice = await eventuallyTrue { released == ["whisper_small_4bit", "whisper_small_4bit"] }
        XCTAssertTrue(twice)
        engine.startError = .noMicrophone
        trial.start(model: "whisper_small_4bit")
        _ = await eventuallyTrue { trial.phase == .failed(DictationError.noMicrophone.message) }
        let thrice = await eventuallyTrue { released.count == 3 }
        XCTAssertTrue(thrice)
    }

    /// Annulé (un modèle supprimé, un autre choisi) : rien n'est enregistré,
    /// même quand la fin du moteur arrive après.
    func testACancelledTrialNeverSavesLater() async {
        let trial = makeTrial()
        trial.start(model: "parakeet_tdt_v3")
        _ = await eventuallyTrue { trial.phase == .listening }
        engine.finalText = "bonjour"
        trial.stop()
        trial.cancel()
        trial.forget("parakeet_tdt_v3")
        try? await Task.sleep(for: .milliseconds(100))
        XCTAssertNil(trial.results["parakeet_tdt_v3"])
        XCTAssertNil(TrialResultsStore(defaults: defaults).all()["parakeet_tdt_v3"])
    }

    func testVoiceOverHearsTheTrialSteps() {
        XCTAssertEqual(MicTrialBlock.spoken(.listening), "Parlez maintenant.")
        XCTAssertEqual(MicTrialBlock.spoken(.finishing), "Le texte final arrive.")
        XCTAssertEqual(MicTrialBlock.spoken(.failed("Aucun micro branché")), "Aucun micro branché")
        XCTAssertNil(MicTrialBlock.spoken(.failed(MicTrialController.noFinal)), "dit par l'annonce de fin")
        XCTAssertNil(MicTrialBlock.spoken(.idle))
    }

    /// L'historique nomme chaque valeur, en entier.
    func testTheHistorySaysEachValueInFull() {
        let r = TrialResult(date: Date(), language: "fr", freeText: false, precision: 92, firstDelay: 0.8,
                            firstDelayFromSpeech: false, finalDelay: 0.3, speed: nil, memoryBytes: 800 << 20,
                            memoryMeasured: false)
        XCTAssertEqual(MicTrialHistory.parts(r, size: { "\($0 >> 20) Mo" }), [
            "Précision : 92 %", "Délai : 0,8 s puis 0,3 s (depuis l'ouverture du micro)", "Débit : —",
            "Mémoire : environ 800 Mo",
        ])
    }
}

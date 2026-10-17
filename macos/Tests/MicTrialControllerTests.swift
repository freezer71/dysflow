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
                                   computeSeconds: { [unowned self] in compute })
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
        trial.stop()
        _ = await eventuallyTrue { trial.phase == .idle }
        trial.forget("system")
        XCTAssertNil(trial.results["system"])
        XCTAssertNil(TrialResultsStore(defaults: defaults).all()["system"])
    }
}

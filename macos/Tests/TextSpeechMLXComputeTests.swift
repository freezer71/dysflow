import XCTest
@testable import Dysflow

/// Un modèle factice : jamais MLX dans les tests unitaires.
final class FakeTextModel: TextSpeechModel {
    let name: String
    let sampleRate: Int
    var stopsOnTheWay = false
    private(set) var spoken: [(String, String, String)] = []

    init(name: String, sampleRate: Int = 22_050) {
        self.name = name
        self.sampleRate = sampleRate
    }

    func synthesize(text: String, language: String, voice: String,
                    shouldStop: @escaping () -> Bool) throws -> [Float]? {
        spoken.append((text, language, voice))
        if stopsOnTheWay, shouldStop() { return nil }
        return [Float](repeating: 0.25, count: sampleRate / 10)
    }
}

final class TextSpeechMLXComputeTests: XCTestCase {
    final class Loads: @unchecked Sendable {
        var count: [String: Int] = [:]
        var models: [String: FakeTextModel] = [:]
        var cleared = 0
    }

    func compute(_ loads: Loads, budget: MLXBudget = MLXBudget(apply: { _, _ in })) -> TextSpeechMLXCompute {
        TextSpeechMLXCompute(
            loader: { url, runtime in
                guard runtime == TextSpeechMLXCompute.qwenRuntime || runtime == TextSpeechMLXCompute.magpieRuntime else {
                    throw DysflowError.Failed(message: "inconnu")
                }
                loads.count[url.path, default: 0] += 1
                let model = FakeTextModel(name: url.path)
                loads.models[url.path] = model
                return model
            },
            clearCache: { loads.cleared += 1 },
            gate: MLXGate(), budget: budget)
    }

    func testLoadsOncePerDirectoryAndReleasesAtTheLastUser() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertEqual(loads.count["/m/q"], 1)
        c.release(modelDir: "/m/q")
        XCTAssertEqual(loads.cleared, 0, "un détenteur reste")
        XCTAssertNoThrow(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "vivian",
                                          cancel: SynthesisCancel.fixed(cancelled: false)))
        c.release(modelDir: "/m/q")
        XCTAssertEqual(loads.cleared, 1)
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              cancel: SynthesisCancel.fixed(cancelled: false)))
    }

    /// Review Focus 2 : la lecture et l'aperçu tiennent chacun leur modèle.
    func testTwoDirectoriesKeepTheirOwnModel() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q17", runtime: "qwen3_tts_mlx")
        try c.load(modelDir: "/m/magpie", runtime: "magpie_mlx")
        _ = try c.synthesize(modelDir: "/m/magpie", text: "aperçu", language: "fr", voice: "leo",
                             cancel: SynthesisCancel.fixed(cancelled: false))
        c.release(modelDir: "/m/magpie")
        let out = try c.synthesize(modelDir: "/m/q17", text: "lecture", language: "fr", voice: "vivian",
                                   cancel: SynthesisCancel.fixed(cancelled: false))
        XCTAssertEqual(loads.models["/m/q17"]?.spoken.map(\.0), ["lecture"])
        XCTAssertEqual(loads.models["/m/magpie"]?.spoken.map(\.0), ["aperçu"])
        XCTAssertEqual(out.sampleRate, 22_050)
    }

    func testCancelledBeforeTheRunIsCancelledNotFailed() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              cancel: SynthesisCancel.fixed(cancelled: true))) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(loads.models["/m/q"]?.spoken.count, 0, "rien calculé")
    }

    func testStoppedOnTheWayIsCancelled() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "magpie_mlx")
        loads.models["/m/q"]?.stopsOnTheWay = true
        var calls = 0
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              isCancelled: { calls += 1; return calls > 1 })) { error in
            guard case DysflowError.Cancelled = error else { return XCTFail("\(error)") }
        }
    }

    func testAnUnknownRuntimeFailsWithItsName() {
        let c = compute(Loads())
        XCTAssertThrowsError(try c.load(modelDir: "/m/x", runtime: "autre")) { error in
            guard case let DysflowError.Failed(message) = error else { return XCTFail("\(error)") }
            XCTAssertTrue(message.hasSuffix("inconnu"), message)
            XCTAssertFalse(message.contains("DysflowError"), "pas d'habit d'erreur dans le message : \(message)")
        }
        XCTAssertEqual(TextSpeechMLXCompute.message(of: DysflowError.Failed(message: "moteur de lecture inconnu : autre")),
                       "moteur de lecture inconnu : autre")
    }

    /// Règle de `MLXBudget` : un chargement raté ne laisse pas son besoin.
    func testAFailedLoadLeavesNoNeedAndCanBeRetried() throws {
        let budget = MLXBudget(apply: { _, _ in })
        let loads = Loads()
        let c = compute(loads, budget: budget)
        XCTAssertThrowsError(try c.load(modelDir: "/m/q", runtime: "autre"))
        XCTAssertEqual(budget.total.memory, 0)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertEqual(loads.count["/m/q"], 1)
        c.release(modelDir: "/m/q")
        XCTAssertEqual(budget.total.memory, 0)
    }

    /// Un `release` de trop, ou d'un dossier jamais chargé : sans effet.
    func testAnExtraReleaseDoesNothing() throws {
        let loads = Loads()
        let c = compute(loads)
        c.release(modelDir: "/m/q")
        try c.load(modelDir: "/m/q", runtime: "magpie_mlx")
        c.release(modelDir: "/m/q")
        c.release(modelDir: "/m/q")
        XCTAssertEqual(loads.cleared, 1)
    }

    /// Une erreur levée après l'abandon reste un abandon ; sans abandon,
    /// c'est un échec avec le message du modèle.
    func testErrorsOfASynthesisAreMapped() {
        struct Boom: LocalizedError { var errorDescription: String? { "boum" } }
        XCTAssertEqual(TextSpeechMLXCompute.synthesisError(Boom(), cancelled: true), .Cancelled)
        XCTAssertEqual(TextSpeechMLXCompute.synthesisError(DysflowError.Cancelled, cancelled: false), .Cancelled)
        XCTAssertEqual(TextSpeechMLXCompute.synthesisError(Boom(), cancelled: false), .Failed(message: "boum"))
        XCTAssertEqual(TextSpeechMLXCompute.synthesisError(DysflowError.Failed(message: "voix inconnue"), cancelled: false),
                       .Failed(message: "voix inconnue"))
    }

    func testTheWarmUpSpeaksWithTheDefaultVoice() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.warmUp(modelDir: "/m/q")
        XCTAssertEqual(loads.models["/m/q"]?.spoken.first?.0, TextSpeechMLXCompute.warmUpText)
        XCTAssertEqual(loads.models["/m/q"]?.spoken.first?.2, "")
    }

    /// Le même dossier déjà chargé et préparé : pas de deuxième synthèse ;
    /// libéré puis rechargé, il se prépare de nouveau.
    func testTheWarmUpRunsOncePerLoadedDirectory() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.warmUp(modelDir: "/m/q")
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.warmUp(modelDir: "/m/q")
        XCTAssertEqual(loads.models["/m/q"]?.spoken.count, 1, "déjà préparé")
        c.release(modelDir: "/m/q")
        c.release(modelDir: "/m/q")
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        try c.warmUp(modelDir: "/m/q")
        XCTAssertEqual(loads.count["/m/q"], 2)
        XCTAssertEqual(loads.models["/m/q"]?.spoken.count, 1, "le nouveau modèle se prépare")
    }

    /// Une synthèse abandonnée ne compte pas comme préparation.
    func testACancelledSynthesisIsNotAWarmUp() throws {
        let loads = Loads()
        let c = compute(loads)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertThrowsError(try c.synthesize(modelDir: "/m/q", text: "x", language: "fr", voice: "",
                                              cancel: SynthesisCancel.fixed(cancelled: true)))
        try c.warmUp(modelDir: "/m/q")
        XCTAssertEqual(loads.models["/m/q"]?.spoken.map(\.0), [TextSpeechMLXCompute.warmUpText])
    }

    func testTheNeedOfEachModelIsInTheBudgetWhileLoaded() throws {
        let budget = MLXBudget(apply: { _, _ in })
        let c = compute(Loads(), budget: budget)
        try c.load(modelDir: "/m/q", runtime: "qwen3_tts_mlx")
        XCTAssertEqual(budget.total, TextSpeechMLXCompute.need(runtime: "qwen3_tts_mlx", weightBytes: 0))
        c.release(modelDir: "/m/q")
        XCTAssertEqual(budget.total.memory, 0)
    }

    func testNeedsFollowTheSurvey() {
        let qwen = TextSpeechMLXCompute.need(runtime: "qwen3_tts_mlx", weightBytes: 3_075_602_023)
        XCTAssertEqual(qwen.memory, 3_075_602_023 + (1 << 30), "1.7B : ≈ 4,1 Go pour un pic mesuré de 3,5–3,7 Go")
        let magpie = TextSpeechMLXCompute.need(runtime: "magpie_mlx", weightBytes: 301_517_780)
        XCTAssertEqual(magpie.memory, 4 * 301_517_780 + (512 << 20), "8 bits déquantifiés en FP32")
    }

    /// Le pont généré par UniFFI, de bout en bout, sur un modèle factice.
    func testTheComputeCrossesTheBridge() throws {
        let c = compute(Loads())
        XCTAssertEqual(try textSpeechBridgeCheck(compute: c, modelDir: "/m/q", runtime: "magpie_mlx"), 2_205)
    }
}

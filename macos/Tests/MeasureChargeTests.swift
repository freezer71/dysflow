import XCTest
@testable import Sillage

/// Le scénario de charge ne tourne jamais sous les tests (il parle à voix
/// haute) ; on vérifie seulement ce dont il dépend : le texte long a assez de
/// phrases pour les sauts 10…50.
@MainActor
final class MeasureChargeTests: XCTestCase {
    func testTheLongTextHoldsTheJumpTargets() async {
        let reader = SillageReader.silent()
        let avant = reader.snapshot().revision
        reader.openText(markdown: Measure.longText, source: "test-charge")
        let ouvert = await eventuallyTrue { reader.snapshot().revision != avant }
        XCTAssertTrue(ouvert)
        reader.command(command: .seekTo(sentence: 50))
        let arrive = await eventuallyTrue { reader.snapshot().sentence == 50 }
        XCTAssertTrue(arrive, "la phrase 50 doit exister dans le texte de charge")
    }

    /// Une voix « prête » d'un autre moteur (un moteur qui ne
    /// sait pas lire le `.safetensors`, muet) ne passe pas pour Kokoro MLX.
    func testOnlyAReadyKokoroMLXVoiceIsAccepted() async {
        let reader = SillageReader.silent()
        reader.openText(markdown: "Un. Deux.", source: "test-kokoro")
        let prete = await eventuallyTrue { reader.snapshot().voice == .ready }
        XCTAssertTrue(prete)
        var s = reader.snapshot()
        XCTAssertFalse(s.engine.isEmpty, "le moteur se nomme")
        XCTAssertFalse(Measure.voixKokoroMLXPrete(s), "le lecteur muet n'est pas Kokoro MLX")
        s.engine = Measure.moteurKokoroMLX
        XCTAssertTrue(Measure.voixKokoroMLXPrete(s))
        s.voice = .fallback(reason: "x")
        XCTAssertFalse(Measure.voixKokoroMLXPrete(s))
        s.voice = .unavailable(reason: "x")
        XCTAssertFalse(Measure.voixKokoroMLXPrete(s))
    }

    func testTheBridgeSurvivesALoweringAndALift() throws {
        let sortie = KokoroOutput(samples: [Float](repeating: 0.5, count: 1000), durations: [2, 3])
        let rendu = try FfiConverterTypeKokoroOutput_lift(FfiConverterTypeKokoroOutput_lower(sortie))
        XCTAssertEqual(rendu, sortie)
    }

    func testTheComputeCountsForwardTime() throws {
        let factory = ForwardFactory()
        let compute = factory.compute()
        try compute.load(modelDir: "/m/kokoro")
        XCTAssertEqual(compute.forwardSeconds, 0)
        _ = try compute.synthesize(ids: [1, 2, 3], style: [])
        XCTAssertEqual(compute.calls, 1)
        XCTAssertGreaterThanOrEqual(compute.forwardSeconds, 0)
    }
}

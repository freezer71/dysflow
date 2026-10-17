import XCTest
@testable import Sillon

/// La mesure de cadence ne tourne jamais sous les tests (elle charge un
/// modèle de plusieurs Go) ; on vérifie la règle qui en tire la vitesse tenue
/// et la phrase de la fiche.
final class MeasureRTFTests: XCTestCase {
    func testTheFluentSpeedKeepsTwentyPercentOfMargin() {
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.25), 3)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.40), 2)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.60), 1.25)
        XCTAssertEqual(Measure.fluentSpeed(rtf: 0.79), 1)
        XCTAssertNil(Measure.fluentSpeed(rtf: 0.85))
    }

    func testThePaceSentenceSaysTheMeasuredFigures() {
        XCTAssertEqual(Measure.paceSentence(rtf: 0.60),
                       "Lourd : lecture hachée au-delà de 1,25× (occupé 60 % du temps à 1× sur ce Mac).")
        XCTAssertEqual(Measure.paceSentence(rtf: 0.25),
                       "Quatre fois plus vite que la parole sur ce Mac : lecture fluide jusqu'à 3×.")
        XCTAssertEqual(Measure.paceSentence(rtf: 0.90),
                       "Lourd : lecture hachée même à 1× (occupé 90 % du temps sur ce Mac).")
    }
}

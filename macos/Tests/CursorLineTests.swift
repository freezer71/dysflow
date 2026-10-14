import XCTest
@testable import Sillon

/// Le trait sous la ligne lue : du début de la ligne au point de la voix.
final class CursorLineTests: XCTestCase {
    private let word = CGRect(x: 100, y: 40, width: 60, height: 36)

    func testTheLineRunsFromTheLineStartToTheVoice() {
        let r = CursorLine.rect(word: word, lineStartX: 12, progress: 0.5, reduceMotion: false)
        XCTAssertEqual(r.minX, 12)
        XCTAssertEqual(r.maxX, 130)
        XCTAssertEqual(r.height, CursorLine.thickness)
        // Sous la ligne de base, dans le bas du mot.
        XCTAssertEqual(r.minY, word.maxY - CursorLine.lift)
        XCTAssertLessThanOrEqual(r.maxY, word.maxY)
    }

    func testAtTheStartOfALineNothingIsDrawnYet() {
        let r = CursorLine.rect(word: word, lineStartX: word.minX, progress: 0, reduceMotion: false)
        XCTAssertEqual(r.width, 0)
    }

    func testTheProgressStaysWithinTheWord() {
        XCTAssertEqual(CursorLine.rect(word: word, lineStartX: 12, progress: 1.7,
                                       reduceMotion: false).maxX, word.maxX)
        XCTAssertEqual(CursorLine.rect(word: word, lineStartX: 12, progress: -0.2,
                                       reduceMotion: false).maxX, word.minX)
        XCTAssertEqual(CursorLine.rect(word: word, lineStartX: 12, progress: .nan,
                                       reduceMotion: false).maxX, word.minX)
    }

    /// Réduire les animations : le trait avance mot par mot, jusqu'à la fin
    /// du mot lu, sans glisser dedans.
    func testReduceMotionJumpsToTheEndOfTheWord() {
        for progress in [0, 0.3, 1] as [CGFloat] {
            let r = CursorLine.rect(word: word, lineStartX: 12, progress: progress, reduceMotion: true)
            XCTAssertEqual(r.maxX, word.maxX)
        }
    }

    /// Un début de ligne mal mesuré, à droite du mot, ne retourne pas le
    /// trait.
    func testTheLineNeverStartsAfterTheWord() {
        let r = CursorLine.rect(word: word, lineStartX: 400, progress: 0.5, reduceMotion: false)
        XCTAssertEqual(r.minX, word.minX)
        XCTAssertEqual(r.maxX, 130)
    }
}

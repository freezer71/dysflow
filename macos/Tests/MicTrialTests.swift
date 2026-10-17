import XCTest
@testable import Sillon

/// La précision de l'essai du micro : 100 − taux d'erreur sur les mots,
/// après normalisation.
final class TrialScoreTests: XCTestCase {
    func testWordsAreLowercasedAndStrippedOfPunctuation() {
        XCTAssertEqual(TrialScore.words("Le Chat, dort !"), ["le", "chat", "dort"])
    }

    func testFrenchQuotesEllipsisAndDashesAreNotWords() {
        XCTAssertEqual(TrialScore.words("« Bonjour » … — dit-il."), ["bonjour", "dit", "il"])
    }

    func testCurlyAndStraightApostrophesSplitTheSameWay() {
        XCTAssertEqual(TrialScore.words("L’été"), TrialScore.words("l'été"))
        XCTAssertEqual(TrialScore.words("l'été"), ["l", "été"])
    }

    func testAccentsAreComparedAfterUnicodeNormalization() {
        // « é » précomposé contre « e » + accent combinant.
        XCTAssertEqual(TrialScore.words("\u{E9}t\u{E9}"), TrialScore.words("e\u{301}te\u{301}"))
        XCTAssertNotEqual(TrialScore.words("été"), TrialScore.words("ete"), "un accent manquant est une erreur")
    }

    func testSpacesCollapse() {
        XCTAssertEqual(TrialScore.words("  un   deux\n trois "), ["un", "deux", "trois"])
    }

    func testNumbersStayAsWritten() {
        XCTAssertEqual(TrialScore.words("Il a 3 chats"), ["il", "a", "3", "chats"])
        XCTAssertLessThan(TrialScore.precision(reference: "trois chats", heard: "3 chats"), 100)
    }

    func testIdenticalSentenceIsAHundred() {
        XCTAssertEqual(TrialScore.precision(reference: "Le chat dort.", heard: "le chat dort"), 100)
    }

    func testOneSubstitutedWordOutOfFour() {
        XCTAssertEqual(TrialScore.errors(reference: ["a", "b", "c", "d"], heard: ["a", "x", "c", "d"]), 1)
        XCTAssertEqual(TrialScore.precision(reference: "le chat dort bien", heard: "le chien dort bien"), 75)
    }

    func testOneDeletedWord() {
        XCTAssertEqual(TrialScore.precision(reference: "le chat dort bien", heard: "le chat bien"), 75)
    }

    func testOneInsertedWord() {
        XCTAssertEqual(TrialScore.precision(reference: "le chat dort bien", heard: "le gros chat dort bien"), 75)
    }

    func testNothingHeardIsZero() {
        XCTAssertEqual(TrialScore.precision(reference: "le chat dort", heard: ""), 0)
    }

    func testNeverBelowZero() {
        XCTAssertEqual(TrialScore.precision(reference: "oui", heard: "non merci beaucoup vraiment"), 0)
    }
}

final class TrialTimelineTests: XCTestCase {
    func testFirstTextIsTimedFromSpeechOnsetWhenHeard() {
        var t = TrialTimeline()
        t.listening = 10
        t.speechOnset = 11
        t.firstText = 11.8
        let delay = t.firstTextDelay
        XCTAssertEqual(delay?.seconds ?? -1, 0.8, accuracy: 1e-9)
        XCTAssertEqual(delay?.basis, .speech)
    }

    func testFirstTextIsTimedFromMicOpenWithoutSpeechOnset() {
        var t = TrialTimeline()
        t.listening = 10
        t.firstText = 12.5
        XCTAssertEqual(t.firstTextDelay?.seconds ?? -1, 2.5, accuracy: 1e-9)
        XCTAssertEqual(t.firstTextDelay?.basis, .micOpen)
    }

    func testAnOnsetAfterTheFirstTextIsNotTrusted() {
        var t = TrialTimeline()
        t.listening = 10
        t.speechOnset = 13
        t.firstText = 12
        XCTAssertEqual(t.firstTextDelay?.basis, .micOpen)
        XCTAssertEqual(t.firstTextDelay?.seconds ?? -1, 2, accuracy: 1e-9)
    }

    func testNoTextNoDelay() {
        var t = TrialTimeline()
        t.listening = 10
        XCTAssertNil(t.firstTextDelay)
    }

    func testFinalDelayIsFromStopToFinalText() {
        var t = TrialTimeline()
        t.stop = 20
        t.final = 20.4
        XCTAssertEqual(t.finalDelay ?? -1, 0.4, accuracy: 1e-9)
        t.final = nil
        XCTAssertNil(t.finalDelay)
    }

    func testAudioIsFromMicOpenToStop() {
        var t = TrialTimeline()
        t.listening = 10
        t.stop = 16
        XCTAssertEqual(t.audioSeconds ?? -1, 6, accuracy: 1e-9)
    }

    func testSpeedIsAudioOverCompute() {
        XCTAssertEqual(TrialTimeline.speed(audioSeconds: 6, computeSeconds: 1.5) ?? -1, 4, accuracy: 1e-9)
        XCTAssertNil(TrialTimeline.speed(audioSeconds: 6, computeSeconds: 0))
        XCTAssertNil(TrialTimeline.speed(audioSeconds: 0, computeSeconds: 1))
    }
}

final class SpeechOnsetTests: XCTestCase {
    private let quiet: [Float] = Array(repeating: 0.05, count: 12)
    private var loud: [Float] { var c = quiet; c[2] = 0.9; return c }

    func testSilenceIsNotSpeech() {
        var d = SpeechOnsetDetector()
        XCTAssertFalse(d.push(Array(repeating: quiet, count: 40)))
        XCTAssertFalse(d.heard)
    }

    func testABriefClickIsNotSpeech() {
        var d = SpeechOnsetDetector()
        XCTAssertFalse(d.push([quiet, loud, quiet, loud, quiet]))
    }

    func testSustainedVoiceIsSpeechOnce() {
        var d = SpeechOnsetDetector()
        XCTAssertFalse(d.push([quiet, loud]))
        XCTAssertTrue(d.push([loud, loud]))
        XCTAssertTrue(d.heard)
        XCTAssertFalse(d.push([loud, loud, loud]), "le début n'est dit qu'une fois")
    }
}

final class TrialMemoryTests: XCTestCase {
    func testAMeasuredGrowthWins() {
        XCTAssertEqual(TrialMemory.reading(before: 100 << 20, after: 900 << 20, estimate: 1 << 30),
                       TrialMemory(bytes: 800 << 20, measured: true))
    }

    func testNoGrowthFallsBackToTheEstimate() {
        XCTAssertEqual(TrialMemory.reading(before: 900 << 20, after: 900 << 20, estimate: 1 << 30),
                       TrialMemory(bytes: 1 << 30, measured: false))
        XCTAssertEqual(TrialMemory.reading(before: nil, after: nil, estimate: 1 << 30),
                       TrialMemory(bytes: 1 << 30, measured: false))
    }

    func testNothingKnownIsNil() {
        XCTAssertNil(TrialMemory.reading(before: nil, after: nil, estimate: 0))
    }
}

final class TrialSentencesTests: XCTestCase {
    func testEachLanguageHasSeveralShortSentences() {
        for language in ["fr", "en"] {
            let list = TrialSentences.list(language)
            XCTAssertGreaterThanOrEqual(list.count, 3)
            XCTAssertTrue(list.allSatisfy { TrialScore.words($0).count <= 14 })
        }
        XCTAssertNotEqual(TrialSentences.list("fr"), TrialSentences.list("en"))
    }

    func testTheSentenceRotates() {
        let n = TrialSentences.list("fr").count
        XCTAssertNotEqual(TrialSentences.sentence("fr", trial: 0), TrialSentences.sentence("fr", trial: 1))
        XCTAssertEqual(TrialSentences.sentence("fr", trial: 0), TrialSentences.sentence("fr", trial: n))
    }

    func testAnUnknownLanguageIsFrench() {
        XCTAssertEqual(TrialSentences.list("de"), TrialSentences.list("fr"))
    }
}

final class TrialResultsStoreTests: XCTestCase {
    private var defaults: UserDefaults!
    private let suite = "MicTrialTests"

    override func setUp() {
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    private func result(_ precision: Double?) -> TrialResult {
        TrialResult(date: Date(timeIntervalSince1970: 1_800_000_000), language: "fr", freeText: precision == nil,
                    precision: precision, firstDelay: 0.8, firstDelayFromSpeech: true, finalDelay: 0.3,
                    speed: 4, memoryBytes: 800 << 20, memoryMeasured: true)
    }

    func testASavedResultIsReadBack() {
        let store = TrialResultsStore(defaults: defaults)
        store.save(result(92), for: "parakeet_tdt_v3")
        XCTAssertEqual(TrialResultsStore(defaults: defaults).all()["parakeet_tdt_v3"], result(92))
    }

    func testTheLastTrialReplacesThePrevious() {
        let store = TrialResultsStore(defaults: defaults)
        store.save(result(50), for: "system")
        store.save(result(nil), for: "system")
        XCTAssertEqual(store.all()["system"], result(nil))
    }

    func testClearingOneModelKeepsTheOthers() {
        let store = TrialResultsStore(defaults: defaults)
        store.save(result(92), for: "parakeet_tdt_v3")
        store.save(result(80), for: "system")
        store.clear("parakeet_tdt_v3")
        XCTAssertNil(store.all()["parakeet_tdt_v3"])
        XCTAssertEqual(store.all()["system"], result(80))
    }

    func testUnreadableDataIsEmpty() {
        defaults.set(Data("pas du json".utf8), forKey: TrialResultsStore.key)
        XCTAssertEqual(TrialResultsStore(defaults: defaults).all(), [:])
    }
}

final class TrialFormatTests: XCTestCase {
    private let size: (UInt64) -> String = { "\($0 >> 20) Mo" }

    private func result() -> TrialResult {
        TrialResult(date: Date(), language: "fr", freeText: false, precision: 91.6, firstDelay: 0.84,
                    firstDelayFromSpeech: true, finalDelay: 0.31, speed: 4.12, memoryBytes: 800 << 20,
                    memoryMeasured: true)
    }

    func testFrenchNumbers() {
        XCTAssertEqual(TrialFormat.percent(91.6), "92 %")
        XCTAssertEqual(TrialFormat.seconds(0.84), "0,8 s")
        XCTAssertEqual(TrialFormat.speed(4.12), "4,1×")
    }

    func testValues() {
        let v = TrialFormat.values(result(), size: size)
        XCTAssertEqual(v.precision, "92 %")
        XCTAssertEqual(v.delay, "0,8 s puis 0,3 s")
        XCTAssertEqual(v.speed, "4,1×")
        XCTAssertEqual(v.memory, "800 Mo")
    }

    func testMissingValuesAreDashes() {
        var r = result()
        r.precision = nil
        r.freeText = true
        r.speed = nil
        r.memoryBytes = nil
        r.firstDelay = nil
        r.finalDelay = nil
        let v = TrialFormat.values(r, size: size)
        XCTAssertEqual(v.precision, "—")
        XCTAssertEqual(v.delay, "—")
        XCTAssertEqual(v.speed, "—")
        XCTAssertEqual(v.memory, "—")
    }

    func testTheAnnouncementSaysEverythingInFullWords() {
        let text = TrialFormat.announcement(result(), model: "Parakeet v3", size: size)
        XCTAssertEqual(text, "Essai de Parakeet v3 terminé. Précision : 92 %. "
            + "Premier texte 0,8 s après le début de la parole. Texte final 0,3 s après l'arrêt. "
            + "Débit : 4,1 fois le temps réel. Mémoire : 800 Mo.")
    }

    func testTheAnnouncementOfAFreeTrialFromMicOpen() {
        var r = result()
        r.freeText = true
        r.precision = nil
        r.firstDelayFromSpeech = false
        r.speed = nil
        r.memoryBytes = nil
        let text = TrialFormat.announcement(r, model: "Dictée de macOS", size: size)
        XCTAssertEqual(text, "Essai de Dictée de macOS terminé. Texte libre : pas de précision. "
            + "Premier texte 0,8 s après l'ouverture du micro. Texte final 0,3 s après l'arrêt.")
    }
}

import XCTest
@testable import Sillon

@MainActor
final class TransportTests: XCTestCase {
    private var sent: [ReaderCommand] = []
    private var current = Snapshot.sample()
    private var doc = DocView(title: "t", complete: true, sentenceCount: 0, sections: [], blocks: [])

    private func transport() -> Transport {
        Transport(snapshot: { [unowned self] in self.current },
                  document: { [unowned self] in self.doc },
                  send: { [unowned self] in self.sent.append($0) })
    }

    /// Un vrai document, découpé par le moteur (lecteur muet).
    private func document(_ text: String) -> DocView {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        return m.document
    }

    private func threeSentences() -> [UInt32] {
        doc = document("Un deux trois quatre. Cinq six. Sept huit neuf dix.")
        let sentences = doc.blocks.flatMap(\.sentences)
        XCTAssertEqual(sentences.map(\.words.count), [4, 2, 4], "découpage attendu par ces tests")
        return sentences.map(\.id)
    }

    func testPlayPauseAndSentences() {
        let t = transport()
        current = .sample(playing: false)
        XCTAssertTrue(t.perform(.toggle))
        XCTAssertTrue(t.perform(.play))
        XCTAssertTrue(t.perform(.pause), "déjà en pause : rien à envoyer, mais servie")
        XCTAssertTrue(t.perform(.next))
        XCTAssertTrue(t.perform(.previous))
        XCTAssertEqual(sent, [.togglePlay, .togglePlay, .sentences(delta: 1), .sentences(delta: -1)])
        sent = []
        current = .sample(playing: true)
        t.perform(.play)
        t.perform(.pause)
        XCTAssertEqual(sent, [.pause], "lecture déjà en cours : seule la pause part")
    }

    func testNothingIsSentWithoutADocument() {
        let t = transport()
        current = .sample(hasDocument: false)
        for action in [RemoteAction.play, .pause, .toggle, .next, .previous, .seek(10)] {
            XCTAssertFalse(t.perform(action), "\(action)")
        }
        XCTAssertEqual(sent, [])
    }

    /// La timeline vise la phrase dont le début est le plus proche (temps
    /// proportionnel aux mots, comme l'estimation du moteur).
    func testTheTimelineLandsOnTheNearestSentence() {
        let ids = threeSentences()
        let s = Snapshot.sample(total: 100)
        XCTAssertEqual(Transport.sentence(at: 0, snapshot: s, document: doc), ids[0])
        XCTAssertEqual(Transport.sentence(at: 35, snapshot: s, document: doc), ids[1])
        XCTAssertEqual(Transport.sentence(at: 55, snapshot: s, document: doc), ids[2])
    }

    func testBeyondTheEndIsTheLastSentenceAndBeforeTheStartTheFirst() {
        let ids = threeSentences()
        let s = Snapshot.sample(total: 100)
        XCTAssertEqual(Transport.sentence(at: 150, snapshot: s, document: doc), ids[2])
        XCTAssertEqual(Transport.sentence(at: 100, snapshot: s, document: doc), ids[2])
        XCTAssertEqual(Transport.sentence(at: -5, snapshot: s, document: doc), ids[0])
    }

    /// « À l'écoute » compte au rythme 1× : à 2×, 70 s visées = 35 % du texte.
    func testTheSpeedIsTakenIntoAccount() {
        let ids = threeSentences()
        XCTAssertEqual(Transport.sentence(at: 70, snapshot: .sample(total: 100, speed: 2), document: doc), ids[1])
    }

    func testWithoutAnEstimateTheTimelineDoesNothing() {
        _ = threeSentences()
        current = .sample(total: nil)
        XCTAssertNil(Transport.sentence(at: 10, snapshot: current, document: doc))
        XCTAssertFalse(transport().perform(.seek(10)))
        XCTAssertNil(Transport.sentence(at: .nan, snapshot: .sample(total: 100), document: doc))
        XCTAssertEqual(sent, [])
    }

    func testSeekSendsTheSentence() {
        let ids = threeSentences()
        current = .sample(total: 100)
        XCTAssertTrue(transport().perform(.seek(150)))
        XCTAssertEqual(sent, [.seekTo(sentence: ids[2])])
    }
}

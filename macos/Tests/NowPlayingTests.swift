import MediaPlayer
import XCTest
@testable import Sillage

final class FakeNowPlayingCenter: NowPlayingCenter {
    var nowPlayingInfo: [String: Any]? { didSet { writes += 1 } }
    var playbackState: MPNowPlayingPlaybackState = .unknown
    private(set) var writes = 0
}

final class FakeRemoteCommands: RemoteCommandSource {
    private(set) var handler: (@MainActor (RemoteAction) -> Bool)?
    private(set) var installs = 0
    private(set) var enabled = false
    func install(_ handler: @escaping @MainActor @Sendable (RemoteAction) -> Bool) {
        installs += 1
        self.handler = handler
    }
    func setEnabled(_ enabled: Bool) { self.enabled = enabled }
}

@MainActor
final class NowPlayingTests: XCTestCase {
    private let center = FakeNowPlayingCenter()
    private let remote = FakeRemoteCommands()
    private let doc = DocView(title: "Les monades", complete: true, sentenceCount: 3, sections: [], blocks: [])
    private var current = Snapshot.sample()
    private var sent: [ReaderCommand] = []
    private var delays: [TimeInterval] = []
    private var cancels = 0
    private var pending: (@MainActor () -> Void)?
    private var refreshes = 0
    /// Ce que la relecture différée voit (par défaut : rien de changé).
    private var onRefresh: (@MainActor () -> Void)?

    private func make(restDelay: TimeInterval? = 602) -> NowPlaying {
        let transport = Transport(snapshot: { [unowned self] in self.current },
                                  document: { [unowned self] in self.doc },
                                  send: { [unowned self] in self.sent.append($0) })
        return NowPlaying(center: center, remote: remote, transport: transport,
                          restDelay: { restDelay },
                          schedule: { [unowned self] delay, work in
                              self.delays.append(delay)
                              self.pending = work
                              return { self.cancels += 1 }
                          },
                          refresh: { [unowned self] in
                              self.refreshes += 1
                              self.onRefresh?()
                          })
    }

    func testPublishesTitleDurationPositionStateAndSpeed() throws {
        let now = make()
        now.update(.sample(playing: true, elapsed: 56, total: 176, speed: 1.5), document: doc)
        let info = try XCTUnwrap(center.nowPlayingInfo)
        XCTAssertEqual(info[MPMediaItemPropertyTitle] as? String, "Les monades")
        XCTAssertEqual(info[MPMediaItemPropertyPlaybackDuration] as? Double, 264, "176 s × 1,5")
        XCTAssertEqual(info[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 84, "56 s × 1,5")
        XCTAssertEqual(info[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 1.5)
        XCTAssertEqual(center.playbackState, .playing)
        XCTAssertTrue(remote.enabled)
        XCTAssertEqual(remote.installs, 1)
    }

    /// Une durée encore inconnue n'est pas inventée.
    func testAnUnknownDurationIsLeftOut() throws {
        let now = make()
        now.update(.sample(elapsed: nil, total: nil), document: doc)
        let info = try XCTUnwrap(center.nowPlayingInfo)
        XCTAssertNil(info[MPMediaItemPropertyPlaybackDuration])
        XCTAssertNil(info[MPNowPlayingInfoPropertyElapsedPlaybackTime])
    }

    /// Le temps qui passe seul ne republie rien : macOS avance la position.
    func testTimePassingAloneRepublishesNothing() {
        let now = make()
        for second in 56...80 {
            now.update(.sample(playing: true, elapsed: Double(second)), document: doc)
        }
        XCTAssertEqual(now.publications, 1)
        now.update(.sample(playing: true, sentence: 1, elapsed: 81), document: doc)
        XCTAssertEqual(now.publications, 2, "nouvelle phrase : position recalée")
        now.update(.sample(playing: false, sentence: 1, elapsed: 81), document: doc)
        XCTAssertEqual(now.publications, 3)
        XCTAssertEqual(center.playbackState, .paused)
        XCTAssertEqual(center.nowPlayingInfo?[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 0)
        now.update(.sample(playing: false, sentence: 1, elapsed: 81, speed: 1.2), document: doc)
        XCTAssertEqual(now.publications, 4, "vitesse changée")
    }

    func testNothingIsShownWithoutADocument() {
        let now = make()
        now.update(.sample(hasDocument: false), document: doc)
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(center.writes, 0)
        XCTAssertEqual(remote.installs, 0)
        XCTAssertFalse(remote.enabled)
    }

    func testTheInfoGoesAwayWhenTheVoiceRests() {
        let now = make()
        now.update(.sample(), document: doc)
        XCTAssertNotNil(center.nowPlayingInfo)
        now.update(.sample(voice: .resting), document: doc)
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(center.playbackState, .stopped)
        XCTAssertFalse(remote.enabled)
        let writes = center.writes
        now.update(.sample(voice: .resting), document: doc)
        XCTAssertEqual(center.writes, writes, "rien à retirer deux fois")
    }

    func testMediaCommandsDriveTheReader() throws {
        let now = make()
        current = .sample(playing: false)
        now.update(current, document: doc)
        let handler = try XCTUnwrap(remote.handler)
        XCTAssertTrue(handler(.toggle))
        XCTAssertTrue(handler(.play))
        XCTAssertTrue(handler(.next))
        XCTAssertTrue(handler(.previous))
        XCTAssertEqual(sent, [.togglePlay, .togglePlay, .sentences(delta: 1), .sentences(delta: -1)])
        current = .sample(hasDocument: false)
        XCTAssertFalse(handler(.toggle), "sans document : ignorée")
        XCTAssertEqual(sent.count, 4)
        // Branchées une seule fois, quel que soit le nombre de publications.
        now.update(.sample(sentence: 2), document: doc)
        XCTAssertEqual(remote.installs, 1)
    }

    /// Une pause prévoit une seule relecture, au moment où le moteur met la
    /// voix au repos ; la lecture qui reprend l'annule.
    func testAPauseWaitsOnceForTheVoiceToRestAndPlayingCancelsIt() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: true), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [], "rien ne se prévoit pendant la lecture")
        now.update(.sample(playing: false), document: doc)
        let armed = await eventuallyTrue { self.delays == [602] }
        XCTAssertTrue(armed)
        now.update(.sample(playing: true, sentence: 1), document: doc)
        XCTAssertEqual(cancels, 1, "la relecture prévue est annulée")
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [602], "une seule relecture prévue")
        XCTAssertEqual(refreshes, 0)
    }

    func testAVoiceKeptForeverSchedulesNothing() async {
        let now = make(restDelay: nil)
        now.update(.sample(playing: false), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [])
    }

    func testRestDelayFollowsKeepTheVoiceLoaded() {
        XCTAssertNil(NowPlaying.restDelay(keepVoiceMinutes: -1), "toujours chargée")
        XCTAssertEqual(NowPlaying.restDelay(keepVoiceMinutes: 0), 2)
        XCTAssertEqual(NowPlaying.restDelay(keepVoiceMinutes: 10), 602)
    }

    /// `ReaderModel.onChange` part à chaque instantané publié, jusqu'à ~17
    /// fois par seconde panneau affiché (avancement au millième, temps à la
    /// seconde) : rien n'est republié tant que seul le temps avance.
    func testManyProgressOnlySnapshotsRepublishNothing() {
        let now = make()
        for tick in 0..<(17 * 30) {
            var s = Snapshot.sample(playing: true, elapsed: 56 + Double(tick / 17), total: 176)
            s.progress = Float(tick) / 10_000
            now.update(s, document: doc)
        }
        XCTAssertEqual(now.publications, 1)
        XCTAssertEqual(center.writes, 1)
    }

    /// Un saut dans la même phrase (la phrase reprise du début, un mot
    /// choisi plus loin) recale la position publiée.
    func testAJumpWithinTheSentenceRepublishes() {
        let now = make()
        now.update(.sample(playing: true, elapsed: 60), document: doc)
        now.update(.sample(playing: true, elapsed: 61), document: doc)
        now.update(.sample(playing: true, elapsed: 62), document: doc)
        XCTAssertEqual(now.publications, 1)
        now.update(.sample(playing: true, elapsed: 56), document: doc)
        XCTAssertEqual(now.publications, 2, "retour au début de la phrase")
        XCTAssertEqual(center.nowPlayingInfo?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 56)
        now.update(.sample(playing: true, elapsed: 57), document: doc)
        XCTAssertEqual(now.publications, 2)
        now.update(.sample(playing: true, elapsed: 70), document: doc)
        XCTAssertEqual(now.publications, 3, "saut en avant dans la phrase")
    }

    /// En pause rien n'avance seul : toute position nouvelle est un saut.
    func testASeekWhilePausedRepublishes() {
        let now = make()
        now.update(.sample(playing: false, elapsed: 56), document: doc)
        now.update(.sample(playing: false, elapsed: 56), document: doc)
        XCTAssertEqual(now.publications, 1)
        now.update(.sample(playing: false, elapsed: 30), document: doc)
        XCTAssertEqual(now.publications, 2)
        XCTAssertEqual(center.nowPlayingInfo?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 30)
    }

    /// La relecture qui trouve la voix encore chargée (une commande en
    /// pause a relancé le compte du moteur, ou « Garder la voix chargée » a
    /// été allongé) en prévoit une autre, une seule à la fois ; celle qui la
    /// trouve au repos retire l'affichage et ne prévoit plus rien.
    func testARereadThatFindsTheVoiceStillLoadedWaitsOnceMore() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: false), document: doc)
        let armed = await eventuallyTrue { self.delays == [602] }
        XCTAssertTrue(armed)
        pending?()
        let rearmed = await eventuallyTrue { self.delays == [602, 602] }
        XCTAssertTrue(rearmed, "toujours en pause, voix chargée : une relecture de plus")
        XCTAssertEqual(cancels, 0, "la première a déjà eu lieu : rien à annuler")
        onRefresh = { now.update(.sample(voice: .resting), document: self.doc) }
        pending?()
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(center.nowPlayingInfo)
        XCTAssertEqual(delays, [602, 602], "au repos : plus rien de prévu")
        XCTAssertEqual(refreshes, 2)
    }

    /// La relecture prévue de nouveau est annulée par la reprise, sans
    /// en créer d'autre.
    func testResumingCancelsTheRearmedReread() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: false), document: doc)
        _ = await eventuallyTrue { self.delays == [602] }
        pending?()
        _ = await eventuallyTrue { self.delays == [602, 602] }
        now.update(.sample(playing: true), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(cancels, 1)
        XCTAssertEqual(delays, [602, 602], "la lecture ne prévoit rien")
    }

    /// Relue entre-temps par un vrai changement d'état (nouvelle pause
    /// publiée), la relecture n'en ajoute pas une seconde : celle du
    /// changement suffit.
    func testARereadThatSeesANewPauseDoesNotDoubleTheTimer() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: false), document: doc)
        _ = await eventuallyTrue { self.delays == [602] }
        onRefresh = { now.update(.sample(playing: false, sentence: 2), document: self.doc) }
        pending?()
        _ = await eventuallyTrue { self.delays.count == 2 }
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [602, 602], "une seule relecture prévue")
    }

    /// Reprendre avant que le délai soit lu (réglages encore en lecture)
    /// n'arme rien après coup.
    func testResumingBeforeTheDelayIsKnownArmsNothing() async {
        let now = make(restDelay: 602)
        now.update(.sample(playing: false), document: doc)
        now.update(.sample(playing: true), document: doc)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(delays, [])
    }

    /// Les deux conventions se répondent : la position publiée (temps estimé
    /// × vitesse, `NowPlayingInfo.from`) ramène, glissée sur la timeline, à
    /// la même phrase (÷ vitesse, `Transport.sentence`).
    func testThePublishedPositionScrubsBackToTheSameSentence() throws {
        let m = ReaderModel(reader: SillageReader.silent())
        m.open(text: "Un deux trois quatre. Cinq six. Sept huit neuf dix.", source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        let document = m.document
        let sentences = document.blocks.flatMap(\.sentences)
        XCTAssertEqual(sentences.map(\.words.count), [4, 2, 4], "découpage attendu")
        // Début de chaque phrase, en temps estimé, sur 100 s : 0, 40, 60.
        let starts: [Double] = [0, 40, 60]
        for speed: Float in [1, 2] {
            for (sentence, start) in zip(sentences, starts) {
                let s = Snapshot.sample(playing: true, sentence: sentence.id, elapsed: start,
                                        total: 100, speed: speed)
                let info = try XCTUnwrap(NowPlayingInfo.from(s, document: document))
                let position = try XCTUnwrap(info.elapsed)
                XCTAssertEqual(Transport.sentence(at: position, snapshot: s, document: document),
                               sentence.id, "×\(speed), phrase \(sentence.id)")
            }
        }
    }
}

import XCTest
@testable import Sillon

@MainActor
final class ReaderModelTests: XCTestCase {
    private func model(_ text: String) -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent())
        m.open(text: text, source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        return m
    }

    func testDocumentFollowsRevision() {
        let m = model("Un. Deux. Trois.")
        XCTAssertEqual(m.document.sentenceCount, 3)
        m.open(text: "Autre.", source: "b")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 2 })
        XCTAssertEqual(m.document.sentenceCount, 1)
    }

    func testSendRefreshesSnapshot() {
        let m = model("Un. Deux. Trois.")
        m.send(.sentences(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.sentence == 1 })
        XCTAssertEqual(m.currentSentenceText, "Deux.")
    }

    /// Le temps et l'avancement changent à chaque image pendant la lecture :
    /// arrondis (seconde, millième), ils ne republient pas l'instantané — et
    /// ne réévaluent pas toute l'interface — soixante fois par seconde.
    func testPublishedSnapshotIsRoundedToWhatTheScreenShows() {
        var a = SillonReader.silent().snapshot()
        a.elapsed = 12.31; a.total = 95.2; a.progress = 0.123_41
        var b = a
        b.elapsed = 12.44; b.total = 95.4; b.progress = 0.123_44
        XCTAssertEqual(ReaderModel.published(a), ReaderModel.published(b))
        XCTAssertEqual(ReaderModel.published(a).elapsed, 12)
        b.elapsed = 13.2
        XCTAssertNotEqual(ReaderModel.published(a), ReaderModel.published(b))
        b = a; b.progress = 0.125
        XCTAssertNotEqual(ReaderModel.published(a), ReaderModel.published(b))
    }

    /// La part du mot dite change à chaque image : elle ne publie rien. La
    /// vue de lecture la relit elle-même pendant la lecture.
    func testPublishedSnapshotIgnoresTheWordProgress() {
        var a = SillonReader.silent().snapshot()
        a.wordProgress = 0.2
        var b = a
        b.wordProgress = 0.7
        XCTAssertEqual(ReaderModel.published(a), ReaderModel.published(b))
        XCTAssertEqual(ReaderModel.published(b).wordProgress, 0)
        b.word = (a.word ?? 0) + 1
        XCTAssertNotEqual(ReaderModel.published(a), ReaderModel.published(b))
    }

    func testLiveSnapshotReadsTheReaderWithoutPublishing() {
        let m = model("Un deux trois.")
        let published = m.snapshot
        XCTAssertEqual(m.liveSnapshot().revision, published.revision)
        XCTAssertEqual(m.snapshot, published)
    }

    /// Sommaire, repères et fil d'Ariane : calculés une fois par révision (et
    /// par section pour le fil), pas à chaque évaluation de la vue.
    func testOutlineIsComputedPerRevision() {
        let m = model("# Monades\n\nUn. Deux.\n\n## Exemples\n\nTrois.")
        XCTAssertEqual(m.tableOfContents.map(\.title), ["Monades", "Exemples"])
        XCTAssertEqual(m.sectionPositions, SectionMarkers.positions(m.document))
        m.send(.sections(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.breadcrumb == "Monades ▸ Exemples" })
        m.open(text: "# Autre\n\nTexte.", source: "b")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 2 })
        XCTAssertEqual(m.tableOfContents.map(\.title), ["Autre"])
        XCTAssertEqual(m.breadcrumb, "Autre")
    }

    func testClearingMessageOnOpen() {
        let m = model("Un.")
        m.message = "Sélectionnez ou copiez du texte"
        m.open(text: "Deux.", source: "b")
        XCTAssertNil(m.message)
    }

    /// Le crochet ne sonne qu'à un instantané publié qui change, pas à
    /// chaque relecture identique.
    func testOnChangeFollowsPublishedChangesOnly() {
        let m = model("Un. Deux. Trois.")
        var sentences: [UInt32] = []
        m.onChange = { s, _ in sentences.append(s.sentence) }
        // Une fois l'état posé (voix prête), deux relectures identiques ne
        // préviennent personne.
        XCTAssertTrue(waitUntil {
            m.refresh()
            let before = sentences.count
            m.refresh()
            return sentences.count == before
        })
        m.send(.sentences(delta: 1))
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.sentence == 1 })
        XCTAssertEqual(sentences.last, 1)
    }
}

@MainActor
final class ReaderModelTickingTests: XCTestCase {
    private var clock = Date()
    /// Les images sont données à la main : un écran en veille n'en envoie pas.
    private let frames = ManualFrames()

    private func hiddenModel() -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent(), now: { [unowned self] in self.clock },
                            frames: frames)
        m.open(text: "Un. Deux.", source: "test")
        XCTAssertTrue(waitUntil { m.refresh(); return m.snapshot.revision == 1 })
        // La période de grâce passée, le modèle masqué au repos s'arrête.
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        return m
    }

    func testIdleHiddenModelDoesNotTick() {
        let m = hiddenModel()
        XCTAssertFalse(m.snapshot.playing)
        XCTAssertFalse(m.isTicking)
    }

    /// Panneau affiché mais en pause : pas de suivi à la cadence de l'écran,
    /// seulement un coup d'œil lent (les messages s'effacent, la voix change
    /// d'état). Masqué : plus rien.
    func testDisplayedButIdleOnlyGlancesThenStopsWhenHidden() {
        let m = hiddenModel()
        m.presence = .full
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .glance)
        m.presence = .hidden
        m.refresh()
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .none)
    }

    func testDisplayedPlayTicksAtScreenRate() {
        let m = hiddenModel()
        m.presence = .full
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { self.frames.tick(); return m.snapshot.playing })
        XCTAssertTrue(m.isTicking)
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { self.frames.tick(); return !m.snapshot.playing })
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .glance)
    }

    /// Toutes les combinaisons : quelque chose avance (lecture, grâce, voix
    /// qui se charge, téléchargement) ou non, pour chaque présence.
    func testCadenceDecision() {
        let running = DownloadView(id: "v", received: 1, total: 10, failed: nil)
        let failed = DownloadView(id: "v", received: 1, total: 10, failed: "réseau")
        let active: [ReaderModel.Cadence] = [.second, .glance, .screen]
        let idle: [ReaderModel.Cadence] = [.none, .none, .glance]
        let presences: [ReaderModel.Presence] = [.hidden, .bar, .full]
        for (i, presence) in presences.enumerated() {
            for playing in [false, true] {
                for grace in [false, true] {
                    for loading in [false, true] {
                        for download in [nil, running, failed] {
                            let moving = playing || grace || loading || download == running
                            XCTAssertEqual(
                                RefreshSchedule.cadence(presence: presence, playing: playing, grace: grace,
                                                    loading: loading, download: download),
                                moving ? active[i] : idle[i],
                                "\(presence) lecture=\(playing) grâce=\(grace) chargement=\(loading) \(String(describing: download))")
                        }
                    }
                }
            }
        }
    }

    /// La relecture par minuterie : la seconde reste en filet quand l'écran
    /// doit donner les images (écran en veille : aucune n'arrive).
    func testPollIntervalForEachCadence() {
        XCTAssertNil(RefreshSchedule.pollInterval(for: .none))
        XCTAssertEqual(RefreshSchedule.pollInterval(for: .second), RefreshSchedule.secondInterval)
        XCTAssertEqual(RefreshSchedule.pollInterval(for: .glance), RefreshSchedule.glanceInterval)
        XCTAssertEqual(RefreshSchedule.pollInterval(for: .screen), RefreshSchedule.secondInterval)
    }

    /// Écran en veille : aucune image n'arrive. La fin de la grâce se voit
    /// quand même, à l'heure — sinon le texte affiché resterait à la cadence
    /// de l'écran, sans jamais relire l'état.
    func testTheGraceEndsWithoutAnyFrame() {
        let silentFrames = ManualFrames()  // jamais d'image
        let m = ReaderModel(reader: SillonReader.silent(), frames: silentFrames)
        m.open(text: "Un. Deux.", source: "test")
        m.presence = .full
        XCTAssertEqual(m.cadence, .screen, "la grâce d'une commande")
        // Moins de 0,8 s : la minuterie de fin de grâce (0,51 s), pas le
        // filet de la seconde (1 s au plus tôt — la tolérance ne fait que
        // retarder une minuterie).
        XCTAssertTrue(waitUntil(0.8) { m.cadence == .glance }, "cadence : \(m.cadence)")
        XCTAssertFalse(silentFrames.isRunning)
    }

    /// En arrière-plan, la lecture se voit sans refresh manuel, à la
    /// seconde, sans lien d'affichage.
    func testHiddenPlayIsSeenWithoutManualRefresh() {
        let m = hiddenModel()
        m.send(.togglePlay)
        XCTAssertEqual(m.cadence, .second)
        XCTAssertFalse(m.isTicking)
        // Aucun refresh manuel : seule la minuterie peut voir `playing`.
        XCTAssertTrue(waitUntil(3) { m.snapshot.playing })
        XCTAssertFalse(m.isTicking)
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil(3) { !m.snapshot.playing })
        // Encore dans la période de grâce : la relecture continue.
        XCTAssertEqual(m.cadence, .second)
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        XCTAssertEqual(m.cadence, .none)
        XCTAssertFalse(m.isTicking)
    }

    func testDownloadInProgressKeepsTicking() {
        let running = DownloadView(id: "v", received: 1, total: 10, failed: nil)
        let failed = DownloadView(id: "v", received: 1, total: 10, failed: "réseau")
        func cadence(_ d: DownloadView?) -> ReaderModel.Cadence {
            RefreshSchedule.cadence(presence: .full, playing: false, grace: false, loading: false, download: d)
        }
        XCTAssertEqual(cadence(running), .screen)
        XCTAssertEqual(cadence(failed), .glance)
        XCTAssertEqual(cadence(nil), .glance)
    }

    func testHiddenModelWithoutDownloadDoesNotTick() {
        let m = hiddenModel()
        XCTAssertNil(m.snapshot.download)
        XCTAssertFalse(m.isTicking)
        m.refresh()
        XCTAssertFalse(m.isTicking)
    }

}

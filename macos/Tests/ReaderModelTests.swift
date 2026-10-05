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

    /// Sommaire, repères et fil d'Ariane : calculés une fois par révision (et
    /// par section pour le fil), pas à chaque évaluation de la vue.
    func testOutlineIsComputedPerRevision() {
        let m = model("# Monades\n\nUn. Deux.\n\n## Exemples\n\nTrois.")
        XCTAssertEqual(m.tableOfContents.map(\.title), ["Monades", "Exemples"])
        XCTAssertEqual(m.sectionPositions, SectionMarkers.positions(m.document))
        m.send(.end)
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
}

@MainActor
final class ReaderModelTickingTests: XCTestCase {
    private var clock = Date()

    private func hiddenModel() -> ReaderModel {
        let m = ReaderModel(reader: SillonReader.silent(), now: { [unowned self] in self.clock })
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
        m.isDisplayed = true
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .glance)
        m.isDisplayed = false
        m.refresh()
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .none)
    }

    func testDisplayedPlayTicksAtScreenRate() {
        let m = hiddenModel()
        m.isDisplayed = true
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { m.snapshot.playing })
        XCTAssertTrue(m.isTicking)
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { !m.snapshot.playing })
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        XCTAssertFalse(m.isTicking)
        XCTAssertEqual(m.cadence, .glance)
    }

    func testCadenceDecision() {
        func cadence(displayed: Bool = false, playing: Bool = false, grace: Bool = false,
                     loading: Bool = false) -> ReaderModel.Cadence {
            ReaderModel.cadence(displayed: displayed, playing: playing, grace: grace,
                                loading: loading, download: nil)
        }
        XCTAssertEqual(cadence(displayed: true), .glance)
        XCTAssertEqual(cadence(), .none)
        XCTAssertEqual(cadence(displayed: true, playing: true), .screen)
        XCTAssertEqual(cadence(displayed: true, grace: true), .screen)
        XCTAssertEqual(cadence(displayed: true, loading: true), .screen)
        // Masqué, une commande ou une lecture se voit quand même.
        XCTAssertEqual(cadence(playing: true), .screen)
        XCTAssertEqual(cadence(grace: true), .screen)
    }

    func testHiddenPlayIsSeenWithoutManualRefresh() {
        let m = hiddenModel()
        m.send(.togglePlay)
        // Aucun refresh manuel : seul le suivi à l'image peut voir `playing`.
        XCTAssertTrue(waitUntil { m.snapshot.playing })
        XCTAssertTrue(m.isTicking)
        m.send(.togglePlay)
        XCTAssertTrue(waitUntil { !m.snapshot.playing })
        // Encore dans la période de grâce : le suivi continue.
        XCTAssertTrue(m.isTicking)
        clock += ReaderModel.gracePeriod + 0.1
        m.refresh()
        XCTAssertFalse(m.isTicking)
    }

    func testDownloadInProgressKeepsTicking() {
        let running = DownloadView(id: "v", received: 1, total: 10, failed: nil)
        let failed = DownloadView(id: "v", received: 1, total: 10, failed: "réseau")
        func cadence(_ d: DownloadView?) -> ReaderModel.Cadence {
            ReaderModel.cadence(displayed: false, playing: false, grace: false, loading: false, download: d)
        }
        XCTAssertEqual(cadence(running), .screen)
        XCTAssertEqual(cadence(failed), .none)
        XCTAssertEqual(cadence(nil), .none)
    }

    func testHiddenModelWithoutDownloadDoesNotTick() {
        let m = hiddenModel()
        XCTAssertNil(m.snapshot.download)
        XCTAssertFalse(m.isTicking)
        m.refresh()
        XCTAssertFalse(m.isTicking)
    }
}

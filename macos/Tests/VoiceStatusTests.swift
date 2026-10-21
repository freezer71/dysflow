import XCTest
@testable import Sillage

/// La ligne d'état de la voix : rien quand tout va bien.
@MainActor
final class VoiceStatusTests: XCTestCase {
    private func snapshot(voice: VoiceState = .ready, download: DownloadView? = nil) -> Snapshot {
        var s = Snapshot.sample(voice: voice)
        s.download = download
        return s
    }

    func testNothingIsSaidWhenTheVoiceIsFine() {
        for voice in [VoiceState.ready, .resting, .loading] {
            XCTAssertNil(VoiceStatus.current(snapshot: snapshot(voice: voice), preparing: false, setupError: nil),
                         "\(voice)")
        }
    }

    func testTheDownloadShowsItsProgress() {
        let d = DownloadView(id: "kokoro_82m_mlx", received: 100_000_000, total: 342_264_783, failed: nil)
        let status = VoiceStatus.current(snapshot: snapshot(voice: .fallback(reason: "Kokoro absent"), download: d),
                                         preparing: false, setupError: nil)
        XCTAssertEqual(status, .downloading(received: 100_000_000, total: 342_264_783))
        XCTAssertEqual(status?.text, "Installation de la voix : 100 Mo sur 342 Mo — la voix du système lit en attendant")
        XCTAssertEqual(status?.canRetry, false)
    }

    /// Un échec, du téléchargement ou de l'appel : « Réessayer ».
    func testAFailureOffersToRetry() {
        let d = DownloadView(id: "kokoro_82m_mlx", received: 5, total: 10, failed: "réseau indisponible")
        let failed = VoiceStatus.current(snapshot: snapshot(download: d), preparing: false, setupError: nil)
        XCTAssertEqual(failed, .failed("réseau indisponible"))
        XCTAssertEqual(failed?.text, "La voix ne s'est pas installée : réseau indisponible")
        XCTAssertEqual(failed?.canRetry, true)
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(), preparing: false, setupError: "disque plein"),
                       .failed("disque plein"))
    }

    func testFallbackUnavailableAndFirstPreparationAreSaid() {
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(voice: .fallback(reason: "Metal indisponible")),
                                           preparing: false, setupError: nil)?.text,
                       "Voix du système en repli : Metal indisponible")
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(voice: .unavailable(reason: "aucune session")),
                                           preparing: false, setupError: nil)?.text,
                       "Voix indisponible : aucune session")
        XCTAssertEqual(VoiceStatus.current(snapshot: snapshot(), preparing: true, setupError: nil)?.text,
                       "Voix en préparation (première fois)…")
    }

    /// Les tests d'interface atteignent le téléchargement et l'échec.
    func testUITestArgumentsStageTheDownloadAndTheFailure() {
        var s = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing-telechargement"])?(&s)
        XCTAssertNotNil(s.download)
        XCTAssertNil(s.download?.failed)
        var f = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing-echec-voix"])?(&f)
        XCTAssertNotNil(f.download?.failed)
    }
}

/// L'installation automatique : son refus se garde, se dit à chaque échec.
@MainActor
final class VoiceInstallTests: XCTestCase {
    private struct Refused: LocalizedError { var errorDescription: String? { "disque plein" } }

    func testAFailureIsKeptAndSaidEachTimeAndSuccessClearsIt() async {
        let model = ReaderModel(reader: SillageReader.silent())
        var said: [String] = []
        model.announceFailure = { said.append($0) }
        await model.installDefaultVoice { throw Refused() }
        XCTAssertEqual(model.voiceSetupError, "disque plein")
        // « Réessayer » échoue de la même façon : dit une seconde fois.
        await model.installDefaultVoice { throw Refused() }
        XCTAssertEqual(said, ["La voix ne s'est pas installée : disque plein",
                              "La voix ne s'est pas installée : disque plein"])
        await model.installDefaultVoice { }
        XCTAssertNil(model.voiceSetupError)
        XCTAssertEqual(said.count, 2, "une réussite ne dit rien")
    }

    /// Le modèle est toujours nommé : ce qui parle, sinon le modèle choisi.
    func testTheModelIsAlwaysNamed() {
        var s = Snapshot.sample(voice: .resting)
        s.engine = ""
        s.model = "Kokoro"
        XCTAssertTrue(ModelName.text(s).contains("Kokoro"), "au repos : le modèle choisi")
        XCTAssertFalse(ModelName.isFallback(s))
        s.voice = .ready
        s.engine = "Kokoro · MLX"
        XCTAssertTrue(ModelName.text(s).contains("Kokoro · MLX"), "ce qui parle")
        XCTAssertFalse(ModelName.isFallback(s))
    }

    /// Un repli se voit : la voix du système nommée, en couleur d'erreur.
    func testAFallbackIsShown() {
        var s = Snapshot.sample(voice: .fallback(reason: "Metal indisponible"))
        s.engine = "système (say) (repli)"
        XCTAssertTrue(ModelName.text(s).contains("système (say) (repli)"))
        XCTAssertTrue(ModelName.isFallback(s))
        s.voice = .unavailable(reason: "aucune session")
        XCTAssertTrue(ModelName.isFallback(s))
    }
}

import XCTest
@testable import Sillon

/// Un calcul Kokoro écrit en Swift, branché sur le pont : jamais MLX dans les
/// tests unitaires.
final class FakeKokoroCompute: KokoroCompute, @unchecked Sendable {
    private let lock = NSLock()
    private var counts = (load: 0, warmUp: 0, synthesize: 0, release: 0)
    var failLoad = false

    var loads: Int { lock.withLock { counts.load } }
    var warmUps: Int { lock.withLock { counts.warmUp } }
    var releases: Int { lock.withLock { counts.release } }

    func load(modelDir: String) throws {
        lock.withLock { counts.load += 1 }
        if failLoad { throw SillonError.Failed(message: "Calcul Metal indisponible") }
    }
    func warmUp() throws { lock.withLock { counts.warmUp += 1 } }
    func synthesize(ids: [Int64], style: [Float]) throws -> KokoroOutput {
        lock.withLock { counts.synthesize += 1 }
        return KokoroOutput(samples: [Float](repeating: 0, count: ids.count * 2 * 600),
                            durations: [UInt32](repeating: 2, count: ids.count))
    }
    func release() { lock.withLock { counts.release += 1 } }
}

final class KokoroBridgeTests: XCTestCase {
    func testASwiftComputeCrossesTheBridge() throws {
        let compute = FakeKokoroCompute()
        XCTAssertEqual(try kokoroBridgeCheck(compute: compute, modelDir: "/x"), 3 * 2 * 600)
        XCTAssertEqual(compute.loads, 1)
        XCTAssertEqual(compute.releases, 1)
    }

    @MainActor
    func testAFailureInSwiftComesBackAsItsMessage() {
        let compute = FakeKokoroCompute()
        compute.failLoad = true
        XCTAssertThrowsError(try kokoroBridgeCheck(compute: compute, modelDir: "/x")) { error in
            XCTAssertEqual(SettingsStore.message(of: error), "Calcul Metal indisponible")
        }
        XCTAssertEqual(compute.releases, 0, "rien de chargé, rien à libérer")
    }

    /// Un geste ne change qu'une langue : c'est elle qui part dans le patch.
    func testAVoicePerLanguageGoesIntoThePatch() {
        let old = SettingsView.defaults
        var new = old
        new.voicesByLanguage.append(LanguageVoice(language: "en", voice: "af_sky"))
        XCTAssertEqual(SettingsDiff.patch(from: old, to: new).voiceForLanguage,
                       LanguageVoice(language: "en", voice: "af_sky"))
        XCTAssertNil(SettingsDiff.patch(from: new, to: new).voiceForLanguage)
    }

    /// Le lecteur muet ne migre ni ne propose rien ; répondre sans
    /// proposition est une erreur dite.
    @MainActor
    func testTheSilentReaderNeitherMigratesNorOffers() async {
        let m = ReaderModel(reader: SillonReader.silent())
        let notice = await m.migrateVoice()
        XCTAssertNil(notice)
        XCTAssertNil(m.snapshot.offer)
        do {
            try await m.acceptVoiceOffer()
            XCTFail("aucune proposition en attente")
        } catch {
            XCTAssertTrue(SettingsStore.message(of: error).contains("aucune proposition"))
        }
        await m.dismissVoiceOffer()
        m.dismissNotice()
        XCTAssertNil(m.snapshot.notice)
        XCTAssertFalse(m.snapshot.configUnreadable)
        XCTAssertFalse(m.voicePreparation.isRunning)
    }

    /// « Remplacer » une configuration illisible passe par le moteur, hors
    /// du fil principal ; le lecteur muet n'écrit rien et le dit.
    @MainActor
    func testReplacingAnUnreadableConfigIsRefusedBySilentReader() async {
        let m = ReaderModel(reader: SillonReader.silent())
        do {
            try await m.replaceUnreadableConfig()
            XCTFail("le lecteur muet n'écrit rien")
        } catch {
            XCTAssertTrue(SettingsStore.message(of: error).contains("mode d'essai"))
        }
    }

    /// Le bandeau des réglages : rien tant que la configuration se lit.
    @MainActor
    func testTheUnreadableConfigNoticeIsShownOnlyWhenNeeded() {
        XCTAssertNil(ConfigUnreadableBanner.line(unreadable: false))
        XCTAssertEqual(ConfigUnreadableBanner.line(unreadable: true),
                       "Configuration illisible : config.toml n'a pas pu être lu. Les réglages s'appliquent, "
                       + "mais ne sont pas enregistrés tant qu'il n'est pas corrigé ou remplacé.")
    }
}

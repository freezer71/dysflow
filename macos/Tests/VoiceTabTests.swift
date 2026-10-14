import XCTest
@testable import Sillon

final class VoiceTabTests: XCTestCase {
    func testKokoroOffersOneVoicePerLanguage() {
        let caps = Capabilities.kokoro
        XCTAssertTrue(VoiceTab.followsLanguage(caps))
        XCTAssertFalse(VoiceTab.followsLanguage(Capabilities.system))
        XCTAssertEqual(VoiceTab.voices(caps, language: "fr").map(\.id), ["p-fenrir60"])
        XCTAssertEqual(VoiceTab.voices(caps, language: "en").map(\.id), ["af_bella", "am_fenrir", "af_heart"])
        var values = SettingsView.defaults
        values.voicesByLanguage = []
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_bella", "rien de choisi : la première")
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "am_fenrir")]
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "am_fenrir")
    }

    /// Kokoro configuré mais en repli (voix du système) ou indisponible : ni
    /// ses voix ni ses « Écouter ». Prêt, au repos, en chargement : oui.
    func testKokoroVoicesShowOnlyWhenKokoroReallySpeaks() {
        let kokoro = Capabilities.kokoro
        for state in [VoiceState.ready, .resting, .loading] {
            XCTAssertTrue(VoiceTab.showsKokoroVoices(kokoro, voice: state), "\(state)")
        }
        XCTAssertFalse(VoiceTab.showsKokoroVoices(kokoro, voice: .fallback(reason: "Metal indisponible")))
        XCTAssertFalse(VoiceTab.showsKokoroVoices(kokoro, voice: .unavailable(reason: "aucune session")))
        XCTAssertFalse(VoiceTab.showsKokoroVoices(Capabilities.system, voice: .ready), "pas Kokoro")
    }

    /// Une voix retirée du catalogue (ou écrite à la main) se montre comme
    /// celle que le moteur fait parler : la recommandée de sa langue. Le menu
    /// ne liste que les voix proposées, sans « (hors liste) ».
    func testARetiredVoiceShowsAsTheRecommendedOne() {
        let caps = Capabilities.kokoro
        var values = SettingsView.defaults
        values.voicesByLanguage = [LanguageVoice(language: "en", voice: "bf_emma"),
                                   LanguageVoice(language: "fr", voice: "ff_siwis")]
        XCTAssertEqual(VoiceTab.voice(for: "en", in: values, caps), "af_bella")
        XCTAssertEqual(VoiceTab.voice(for: "fr", in: values, caps), "p-fenrir60")
        XCTAssertEqual(VoiceTab.voiceOptions(caps, language: "en").map(\.value), ["af_bella", "am_fenrir", "af_heart"])
        XCTAssertEqual(VoiceTab.voiceOptions(caps, language: "fr").map(\.title), ["p-fenrir60 · française, recommandée"])
    }

    func testTheDocumentLanguageSaysWhatWasDetected() {
        let detected = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.title),
                       ["Automatique (français)", "Français", "Anglais"])
        XCTAssertEqual(VoiceTab.documentLanguageOptions(detected).map(\.value), ["", "fr", "en"])
        let unknown = DocumentLanguageView(detected: nil, chosen: nil, effective: "fr", minorityShare: 0, mixed: false)
        XCTAssertEqual(VoiceTab.documentLanguageOptions(unknown).first?.title, "Automatique (non détectée)")
    }

    func testAMixedDocumentIsSaidWithItsMajority() {
        let mixed = DocumentLanguageView(detected: "fr", chosen: "en", effective: "en", minorityShare: 0.38, mixed: true)
        XCTAssertEqual(VoiceTab.mixedLine(mixed), "Document mêlé : 62 % en français")
        let plain = DocumentLanguageView(detected: "fr", chosen: nil, effective: "fr", minorityShare: 0.1, mixed: false)
        XCTAssertNil(VoiceTab.mixedLine(plain))
    }

    func testTheFirstPreparationIsSaid() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: true), "Voix en préparation (première fois)…")
        XCTAssertEqual(VoiceTab.voiceLine(.loading, preparing: false), "Voix en chargement…")
        XCTAssertEqual(VoiceTab.voiceLine(.ready, preparing: true), "Voix en préparation (première fois)…",
                       "la préparation tourne à part du lecteur, voix prête ou au repos")
        XCTAssertEqual(VoiceTab.voiceLine(.resting, preparing: true), "Voix en préparation (première fois)…")
    }

    /// M1 : l'offre en un clic, avec sa taille écrite comme partout dans
    /// l'app (`SizeFormat.bytes`, base 1000, unités françaises).
    func testTheOfferSaysItsSizeTheFrenchWay() {
        let offer = VoiceOfferView(id: "kokoro_82m_mlx", bytes: 342_264_783, former: "F2", message: "")
        XCTAssertEqual(VoiceTab.offerTitle(offer), "Passer à la nouvelle voix (342 Mo)")
        XCTAssertEqual(VoiceTab.launchNotice(offer), "Nouvelle voix disponible (342 Mo) : Réglages › Voix")
    }

    /// Au lancement, une offre se dit à la façon de l'app, jamais avec la
    /// phrase du moteur (base 1024, question sans bouton).
    @MainActor
    func testTheLaunchNoticeOfAnOfferIsTheApps() {
        let offer = VoiceOfferView(id: "kokoro_82m_mlx", bytes: 342_264_783, former: "F2",
                                   message: "Nouvelle voix disponible (326,4 Mo) — Passer à Kokoro ?")
        XCTAssertEqual(AppDelegate.launchNotice(engine: offer.message, offer: offer),
                       "Nouvelle voix disponible (342 Mo) : Réglages › Voix")
        XCTAssertEqual(AppDelegate.launchNotice(engine: "Nouvelle voix : Kokoro remplace F2.", offer: nil),
                       "Nouvelle voix : Kokoro remplace F2.")
        XCTAssertNil(AppDelegate.launchNotice(engine: nil, offer: nil))
    }

    /// Le téléchargement accepté échoue : l'offre revient, et l'échec se dit.
    func testAFailedAcceptedDownloadIsSaid() {
        let failed = DownloadView(id: "kokoro_82m_mlx", received: 1, total: 2, failed: "réseau coupé")
        XCTAssertEqual(VoiceTab.acceptedDownloadFailure(accepted: "kokoro_82m_mlx", download: failed),
                       "Téléchargement interrompu : réseau coupé")
        XCTAssertNil(VoiceTab.acceptedDownloadFailure(accepted: nil, download: failed), "rien d'accepté ici")
        XCTAssertNil(VoiceTab.acceptedDownloadFailure(accepted: "autre", download: failed))
        let going = DownloadView(id: "kokoro_82m_mlx", received: 1, total: 2, failed: nil)
        XCTAssertNil(VoiceTab.acceptedDownloadFailure(accepted: "kokoro_82m_mlx", download: going))
    }

    func testSizesAreWrittenTheFrenchWayInBase1000() {
        XCTAssertEqual(SizeFormat.bytes(342_264_783), "342 Mo")
        XCTAssertEqual(SizeFormat.bytes(92_361_055), "92,4 Mo")
        XCTAssertEqual(SizeFormat.bytes(522_320), "522 Ko")
        XCTAssertEqual(SizeFormat.bytes(1_500_000_000), "1,5 Go")
        XCTAssertEqual(SizeFormat.bytes(5_000_000), "5 Mo")
        XCTAssertEqual(SizeFormat.bytes(999_700), "1 Mo", "jamais « 1 000 Ko »")
        XCTAssertEqual(SizeFormat.bytes(999), "999 octets")
        XCTAssertEqual(SizeFormat.bytes(1), "1 octet")
        XCTAssertEqual(SizeFormat.bytes(0), "0 octet")
        XCTAssertEqual(SizeFormat.bytes(999_499), "999 Ko", "sous la frontière")
        XCTAssertEqual(SizeFormat.bytes(999_500), "1 Mo", "à la frontière : jamais « 1 000 Ko »")
    }

    /// L'avis et l'offre se disent une fois par valeur, pas à chaque retour
    /// sur l'onglet.
    @MainActor
    func testAnAnnouncementIsMadeOncePerValue() {
        var once = AnnounceOnce()
        XCTAssertTrue(once.shouldSay("Nouvelle voix"))
        XCTAssertFalse(once.shouldSay("Nouvelle voix"))
        XCTAssertTrue(once.shouldSay("Autre avis"))
    }

    /// Les tests d'interface (compilés seulement) atteignent l'offre, l'avis
    /// et le bandeau sous le lecteur muet.
    func testUITestArgumentsStageTheOfferNoticeAndUnreadableConfig() {
        XCTAssertNil(UITestStaging.staging(for: ["--ui-testing"]))
        var s = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing", "--ui-testing-offre", "--ui-testing-config-illisible"])?(&s)
        XCTAssertEqual(s.offer?.bytes, 342_264_783)
        XCTAssertTrue(s.configUnreadable)
        XCTAssertNil(s.notice)
        var n = Snapshot.sample()
        UITestStaging.staging(for: ["--ui-testing-avis"])?(&n)
        XCTAssertNotNil(n.notice)
        XCTAssertNil(n.offer)
        XCTAssertNil(UITestStaging.capabilities(for: ["--ui-testing"]))
        let kokoro = UITestStaging.capabilities(for: ["--ui-testing", "--ui-testing-kokoro"])
        XCTAssertEqual(kokoro?.family, "kokoro")
        XCTAssertEqual(kokoro?.voices.map(\.id), Capabilities.kokoro.voices.map(\.id),
                       "les voix du moteur : p-fenrir60, af_bella, am_fenrir, af_heart")
    }

    func testTheVoiceStateLine() {
        XCTAssertEqual(VoiceTab.voiceLine(.loading), "Voix en chargement…")
        XCTAssertNil(VoiceTab.voiceLine(.ready))
        XCTAssertNil(VoiceTab.voiceLine(.resting))
        XCTAssertEqual(VoiceTab.voiceLine(.unavailable(reason: "famille sans synthèse")),
                       "Voix indisponible : famille sans synthèse")
    }

    func testModelsAreSuggestedForAFallbackOrAnUnavailableVoice() {
        XCTAssertTrue(VoiceTab.suggestsModels(.fallback(reason: "x")))
        XCTAssertTrue(VoiceTab.suggestsModels(.unavailable(reason: "x")))
        XCTAssertFalse(VoiceTab.suggestsModels(.ready))
        XCTAssertFalse(VoiceTab.suggestsModels(.loading))
    }

    /// 1c : le repli se dit une seule fois, avec sa raison.
    func testTheFallbackIsSaidOnceWithItsReason() {
        XCTAssertEqual(VoiceTab.voiceLine(.fallback(reason: "fichier manquant")),
                       "Voix du système en repli : fichier manquant")
    }

    func testThePreviewLineSaysWhyFirst() {
        let idle = PreviewView(pending: false, playing: false, error: nil)
        XCTAssertNil(VoiceTab.previewLine(idle, refusal: nil))
        XCTAssertEqual(VoiceTab.previewLine(idle, refusal: "sortie audio indisponible"),
                       "sortie audio indisponible")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: true, playing: false, error: nil), refusal: nil),
                       "Synthèse de l'aperçu…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: true, error: nil), refusal: nil),
                       "Écoute…")
        XCTAssertEqual(VoiceTab.previewLine(PreviewView(pending: false, playing: false, error: "échec"), refusal: nil),
                       "échec")
    }

    func testNumbersAreWrittenTheFrenchWay() {
        XCTAssertEqual(SettingFormat.decimal(1.25), "1,25")
        XCTAssertEqual(SettingFormat.decimal(1), "1")
        XCTAssertEqual(SettingFormat.percent(0.6), "60 %")
        XCTAssertEqual(LanguageName.display("de"), "Allemand")
    }
}

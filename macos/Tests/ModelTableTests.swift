import XCTest
@testable import Dysflow

final class ModelTableTests: XCTestCase {
    func row(_ id: String = "kokoro_82m_mlx", installed: Bool = false, active: Bool = false,
             bytes: UInt64 = 342_000_000, disk: UInt64 = 0, system: Bool = false,
             files: Bool? = nil) -> VoiceModel {
        VoiceModel(id: id, name: system ? "Voix de macOS" : "Kokoro 82M · MLX", bytes: bytes,
                   installed: installed, active: active, recommended: false, summary: "",
                   family: system ? "system" : "kokoro", precision: "bf16", languagesLabel: "Français, anglais",
                   languages: ["fr", "en"], pace: "", voicesLabel: "", licenses: "",
                   kind: "speech", diskBytes: disk, system: system, hasFiles: files ?? installed)
    }

    func testStates() {
        XCTAssertEqual(ModelTable.state(row(system: true), download: nil), .system)
        XCTAssertEqual(ModelTable.state(row(), download: nil), .notInstalled)
        XCTAssertEqual(ModelTable.state(row(installed: true), download: nil), .installed)
        let half = DownloadView(id: "kokoro_82m_mlx", received: 50, total: 100, failed: nil)
        XCTAssertEqual(ModelTable.state(row(), download: half), .downloading(0.5))
        let failed = DownloadView(id: "kokoro_82m_mlx", received: 0, total: 100, failed: "réseau")
        XCTAssertEqual(ModelTable.state(row(), download: failed), .failed("réseau"))
        // Le téléchargement d'un autre modèle ne change pas cette ligne.
        let other = DownloadView(id: "autre", received: 1, total: 2, failed: nil)
        XCTAssertEqual(ModelTable.state(row(), download: other), .notInstalled)
    }

    /// Un téléchargement annulé n'est pas un échec : la ligne redevient
    /// « non installée », sans rouge.
    func testACancelledDownloadIsShownAsNotInstalled() {
        let cancelled = DownloadView(id: "kokoro_82m_mlx", received: 0, total: 100, failed: "annulé")
        XCTAssertEqual(ModelTable.state(row(), download: cancelled), .notInstalled)
        XCTAssertTrue(ModelTable.canDownload(row(), download: cancelled))
    }

    /// Des fichiers laissés par un téléchargement annulé ou échoué se
    /// suppriment depuis la table ; la voix de macOS jamais.
    func testLeftoverFilesCanBeRemoved() {
        XCTAssertTrue(ModelTable.canRemove(row(files: true), .notInstalled))
        XCTAssertTrue(ModelTable.canRemove(row(files: true), .failed("réseau")))
        XCTAssertFalse(ModelTable.canRemove(row(), .notInstalled), "rien sur le disque")
        XCTAssertFalse(ModelTable.canRemove(row(files: true), .downloading(0.5)), "annuler d'abord")
        XCTAssertTrue(ModelTable.canRemove(row(installed: true), .installed))
        XCTAssertFalse(ModelTable.canRemove(row(system: true), .system))
        let partial = ModelTable.confirmation(row(disk: 120_000_000, files: true), fallback: "x")
        XCTAssertTrue(partial.message.hasPrefix("120 Mo seront libérés."), partial.message)
        let label = ModelsTable.label(row(disk: 120_000_000, files: true), .notInstalled)
        XCTAssertTrue(label.hasSuffix("téléchargement incomplet, 120 Mo sur le disque"), label)
    }

    /// VoiceOver dit de chaque ligne son nom, si elle est utilisée, son état
    /// et sa taille.
    func testEachRowSaysItsNameUseStateAndSize() {
        let used = ModelsTable.label(row(installed: true, active: true, disk: 342_000_000), .installed)
        XCTAssertTrue(used.hasPrefix("Kokoro 82M · MLX, utilisé, installé, "), used)
        XCTAssertEqual(ModelsTable.label(row(system: true), .system), "Voix de macOS, non utilisée, sur ce Mac")
        XCTAssertEqual(ModelsTable.label(row(active: true, system: true), .system), "Voix de macOS, utilisée, sur ce Mac")
        XCTAssertTrue(ModelsTable.label(row(installed: true), .installed).hasPrefix("Kokoro 82M · MLX, non utilisé,"))
        XCTAssertTrue(ModelsTable.label(row(), .notInstalled).contains("non installé"))
        XCTAssertTrue(ModelsTable.label(row(), .downloading(0.5)).hasSuffix("téléchargement 50 %"))
    }

    /// Chaque ligne porte le logo de son éditeur ; sans logo, une initiale.
    func testEachModelShowsItsMaker() {
        XCTAssertEqual(ModelTable.maker(row(system: true)), .apple)
        XCTAssertEqual(ModelTable.maker(row("parakeet_tdt_v3")), .nvidia)
        XCTAssertEqual(ModelTable.maker(row("canary_1b_v2_q8")), .nvidia)
        XCTAssertEqual(ModelTable.maker(row("magpie_tts_357m_8bit")), .nvidia)
        XCTAssertEqual(ModelTable.maker(row("whisper_large_v3_turbo_4bit")), .openai)
        XCTAssertEqual(ModelTable.maker(row("voxtral_realtime_4bit")), .mistral)
        XCTAssertEqual(ModelTable.maker(row("qwen3_tts_06b_base_8bit")), .qwen)
        XCTAssertEqual(ModelTable.maker(row("phonon_2")), .initial("F"))
        XCTAssertEqual(ModelTable.maker(row()), .initial("K"))
        XCTAssertNil(ModelMaker.initial("K").logo)
        // Les modèles de dictée du lot 4.
        XCTAssertEqual(ModelTable.maker(row("parakeet_tdt_v2")), .nvidia)
        XCTAssertEqual(ModelTable.maker(row("nemotron_35_asr_streaming_8bit")), .nvidia)
        XCTAssertEqual(ModelTable.maker(row("qwen3_asr_06b_8bit")), .qwen)
        XCTAssertEqual(ModelTable.maker(row("qwen3_asr_17b_4bit")), .qwen)
        XCTAssertEqual(ModelTable.maker(row("cohere_transcribe_8bit")), .cohere)
        XCTAssertEqual(ModelMaker.cohere.logo, "logo-cohere")
    }

    /// Le filtre « Éditeur » et VoiceOver disent l'éditeur en clair, même
    /// quand la pastille n'en montre que l'initiale.
    func testEachMakerHasAReadableName() {
        XCTAssertEqual([ModelMaker.apple, .nvidia, .openai, .mistral, .qwen, .cohere].map(\.name),
                       ["Apple", "NVIDIA", "OpenAI", "Mistral", "Qwen", "Cohere"])
        XCTAssertEqual(ModelMaker.initial("K").name, "hexgrad")
        XCTAssertEqual(ModelMaker.initial("F").name, "Fermion Research")
        XCTAssertEqual(ModelMaker.initial("Z").name, "Z")
    }

    func testOneDownloadAtATime() {
        let running = DownloadView(id: "autre", received: 1, total: 2, failed: nil)
        XCTAssertFalse(ModelTable.canDownload(row(), download: running))
        XCTAssertTrue(ModelTable.canDownload(row(), download: nil))
    }

    func testSizeIsWhatIsOnDiskOnceInstalled() {
        XCTAssertEqual(ModelTable.size(row(system: true)), "—")
        XCTAssertEqual(ModelTable.size(row(installed: true, disk: 342_264_783)), "342 Mo")
        XCTAssertEqual(ModelTable.size(row(bytes: 92_361_055)), "92,4 Mo", "à télécharger")
    }

    /// Les tailles s'écrivent à la française, en base 1000, comme ailleurs
    /// dans les réglages.
    func testSizesUseTheFrenchFormat() {
        XCTAssertEqual(ModelTable.bytes(342_264_783), SizeFormat.bytes(342_264_783))
        XCTAssertEqual(ModelTable.footer(used: 342_264_783, free: 12_500_000_000),
                       "342 Mo utilisés par les modèles · 12,5 Go libres")
    }

    /// « Voix de macOS » est féminin ; un modèle, masculin.
    func testTheMacVoiceIsUsedInTheFeminine() {
        XCTAssertEqual(ModelTable.used(row(system: true)), "utilisée")
        XCTAssertEqual(ModelTable.used(row()), "utilisé")
    }

    func testConfirmationSaysWhatIsFreedAndWhatTakesOver() {
        let inUse = ModelTable.confirmation(row(installed: true, active: true, disk: 342_000_000),
                                            fallback: "La lecture passera à la voix de macOS.")
        XCTAssertEqual(inUse.title, "Supprimer Kokoro 82M · MLX ?")
        XCTAssertTrue(inUse.message.contains("seront libérés"))
        XCTAssertTrue(inUse.message.contains("voix de macOS"))
        let idle = ModelTable.confirmation(row(installed: true, disk: 342_000_000), fallback: "x")
        XCTAssertFalse(idle.message.contains("x."))
    }

    /// VoiceOver entend la progression tous les 25 %, pas à chaque relecture.
    func testProgressIsAnnouncedEveryQuarter() {
        XCTAssertNil(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.10, current: 0.20))
        XCTAssertEqual(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.20, current: 0.26),
                       "Téléchargement de Parakeet v3 : 25 %")
        XCTAssertEqual(ModelTable.progressAnnouncement(name: "Parakeet v3", previous: 0.74, current: 1.0),
                       "Téléchargement de Parakeet v3 : 100 %")
    }

    func testDictationRowsAreUsedFromTheirOwnChoice() {
        // La ligne active de dictée vient de `dictationModel`, pas du moteur Rust.
        let rows = [VoiceModel.sample(id: "parakeet_tdt_v3", kind: "dictation", installed: true)]
        let shown = DictationSection.rows(rows, chosen: "parakeet_tdt_v3")
        XCTAssertEqual(shown.first?.id, "system-dictation")
        XCTAssertTrue(shown[1].active)
        XCTAssertFalse(shown[0].active)
    }

    /// La table Dictée ne montre pas les voix de lecture, et la dictée de
    /// macOS est utilisée tant qu'aucun modèle n'est choisi.
    func testTheDictationTableShowsOnlyDictationModels() {
        let rows = [VoiceModel.sample(id: "kokoro_82m_mlx", kind: "speech", active: true),
                    VoiceModel.sample(id: "whisper_tiny_4bit", kind: "dictation", installed: false, active: true)]
        let shown = DictationSection.rows(rows, chosen: DictationModel.system)
        XCTAssertEqual(shown.map(\.id), ["system-dictation", "whisper_tiny_4bit"])
        XCTAssertEqual(shown.map(\.active), [true, false], "jamais l'actif du moteur Rust")
        XCTAssertTrue(shown[0].system, "la dictée de macOS ne se supprime ni ne se télécharge")
        XCTAssertEqual(ModelTable.used(shown[0]), "utilisée")
    }

    func testAClickOnAMissingModelDownloadsIt() {
        let missing = VoiceModel.sample(id: "parakeet", kind: "dictation", installed: false)
        XCTAssertTrue(ModelTable.downloadsOnClick(missing, download: nil))
        XCTAssertFalse(ModelTable.downloadsOnClick(.sample(id: "kokoro", installed: true), download: nil),
                       "installé : le clic choisit")
        let running = DownloadView(id: "whisper", received: 1, total: 10, failed: nil)
        XCTAssertFalse(ModelTable.downloadsOnClick(missing, download: running), "un seul téléchargement à la fois")
        let cancelled = DownloadView(id: "parakeet", received: 0, total: 10, failed: ModelTable.cancelled)
        XCTAssertTrue(ModelTable.downloadsOnClick(missing, download: cancelled), "après une annulation, on peut relancer")
    }

    func testARowIsChosenAnywhereOnceItsModelIsThere() {
        XCTAssertTrue(ModelTable.canUse(.sample(id: "kokoro", kind: "speech", installed: true)))
        XCTAssertFalse(ModelTable.canUse(.sample(id: "magpie", kind: "speech", installed: false)),
                       "pas installé : rien à choisir")
        var used = VoiceModel.sample(id: "qwen", kind: "speech", installed: true)
        used.active = true
        XCTAssertFalse(ModelTable.canUse(used), "déjà utilisé")
    }


    /// Le rond choisit un modèle là ; une flèche dit qu'un clic le télécharge.
    func testTheChoiceShowsWhatAClickDoes() {
        XCTAssertEqual(ModelsTable.choiceSymbol(row(installed: true, active: true)), "largecircle.fill.circle")
        XCTAssertEqual(ModelsTable.choiceSymbol(row(installed: true)), "circle")
        XCTAssertEqual(ModelsTable.choiceSymbol(row(system: true)), "circle")
        XCTAssertEqual(ModelsTable.choiceSymbol(row()), "arrow.down.circle")
    }
}

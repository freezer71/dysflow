import XCTest
@testable import Dysflow

/// La bibliothèque des modèles de dictée : ses filtres, son ordre, ce que
/// VoiceOver dit d'une ligne, et ce que la table de la section garde.
final class ModelLibraryTests: XCTestCase {
    private func model(_ id: String, _ name: String, installed: Bool = false, languages: [String] = ["fr", "en"],
                       languagesLabel: String = "Français, anglais", bytes: UInt64 = 600_000_000,
                       disk: UInt64 = 0, files: Bool? = nil) -> VoiceModel {
        VoiceModel(id: id, name: name, bytes: bytes, installed: installed, active: false, recommended: false,
                   summary: "Un résumé.", family: "", precision: "8 bits", languagesLabel: languagesLabel,
                   languages: languages, pace: "", voicesLabel: "", licenses: "", kind: "dictation",
                   diskBytes: disk, system: false, hasFiles: files ?? installed)
    }

    /// Les lignes de la feuille, telles que `DictationSection.rows` les donne.
    private func sample(chosen: String = DictationModel.system) -> [VoiceModel] {
        DictationSection.rows([
            model("whisper_large_v3_turbo_4bit", "Whisper Turbo", installed: true),
            model("parakeet_tdt_v2", "Parakeet v2", languages: ["en"], languagesLabel: "Anglais"),
            model("cohere_transcribe_8bit", "Cohere Transcribe"),
            model("parakeet_tdt_v3", "Parakeet v3", installed: true),
            model("nemotron_35_asr_streaming_8bit", "Nemotron 3.5 Streaming"),
            model("qwen3_asr_06b_8bit", "Qwen3-ASR 0.6B"),
            model("voxtral_realtime_4bit", "Voxtral Réel", installed: true),
        ], chosen: chosen)
    }

    /// L'utilisé d'abord, puis ceux qui sont là, puis par éditeur et par nom.
    func testTheUsedModelComesFirstThenTheInstalledOnesThenByMakerAndName() {
        let order = ModelLibrary.ordered(sample(chosen: "voxtral_realtime_4bit")).map(\.id)
        XCTAssertEqual(order, [
            "voxtral_realtime_4bit",
            "system-dictation",               // Apple
            "parakeet_tdt_v3",                // NVIDIA
            "whisper_large_v3_turbo_4bit",    // OpenAI
            "cohere_transcribe_8bit",         // Cohere
            "nemotron_35_asr_streaming_8bit", // NVIDIA, puis par nom
            "parakeet_tdt_v2",
            "qwen3_asr_06b_8bit",             // Qwen
        ])
    }

    func testTheMakerFilterUsesTheReadableName() {
        let rows = sample()
        XCTAssertEqual(ModelLibrary.makerOptions(rows).map(\.title),
                       ["Tous", "Apple", "Cohere", "Mistral", "NVIDIA", "OpenAI", "Qwen"])
        var filter = ModelLibrary.Filter()
        filter.maker = "NVIDIA"
        XCTAssertEqual(ModelLibrary.filtered(rows, filter).map(\.id),
                       ["parakeet_tdt_v3", "nemotron_35_asr_streaming_8bit", "parakeet_tdt_v2"])
    }

    /// « Français » : les modèles qui l'écrivent ; « Anglais seulement » :
    /// ceux qui n'écrivent que lui.
    func testTheLanguageFilter() {
        var filter = ModelLibrary.Filter()
        filter.language = ModelLibrary.englishOnly
        XCTAssertEqual(ModelLibrary.filtered(sample(), filter).map(\.id), ["parakeet_tdt_v2"])
        filter.language = ModelLibrary.french
        XCTAssertFalse(ModelLibrary.filtered(sample(), filter).map(\.id).contains("parakeet_tdt_v2"))
        XCTAssertEqual(ModelLibrary.filtered(sample(), filter).count, sample().count - 1)
        XCTAssertEqual(ModelLibrary.languageOptions.map(\.title), ["Toutes", "Français", "Anglais seulement"])
    }

    /// « Installés » compte la dictée de macOS ; « À télécharger », le reste.
    func testTheStateFilter() {
        var filter = ModelLibrary.Filter()
        filter.state = ModelLibrary.installed
        XCTAssertEqual(Set(ModelLibrary.filtered(sample(), filter).map(\.id)),
                       ["system-dictation", "whisper_large_v3_turbo_4bit", "parakeet_tdt_v3",
                        "voxtral_realtime_4bit"])
        filter.state = ModelLibrary.missing
        XCTAssertEqual(Set(ModelLibrary.filtered(sample(), filter).map(\.id)),
                       ["parakeet_tdt_v2", "cohere_transcribe_8bit", "nemotron_35_asr_streaming_8bit",
                        "qwen3_asr_06b_8bit"])
        XCTAssertEqual(ModelLibrary.stateOptions.map(\.title), ["Tous", "Installés", "À télécharger"])
    }

    /// La recherche porte sur le nom, sans casse ni accents ; les filtres
    /// se cumulent.
    func testSearchIgnoresCaseAndAccentsAndCombinesWithFilters() {
        var filter = ModelLibrary.Filter()
        filter.search = "  reel "
        XCTAssertEqual(ModelLibrary.filtered(sample(), filter).map(\.id), ["voxtral_realtime_4bit"])
        filter.search = "PARAKEET"
        XCTAssertEqual(ModelLibrary.filtered(sample(), filter).count, 2)
        filter.state = ModelLibrary.missing
        XCTAssertEqual(ModelLibrary.filtered(sample(), filter).map(\.id), ["parakeet_tdt_v2"])
        filter.search = "nvidia"
        XCTAssertTrue(ModelLibrary.filtered(sample(), filter).isEmpty, "le nom seulement, pas l'éditeur")
    }

    /// Une ligne cachée par les filtres n'est pas utilisée ; « Utiliser » ne
    /// prend qu'un modèle qui est là.
    func testOnlyAVisibleInstalledModelCanBeUsed() {
        var filter = ModelLibrary.Filter()
        filter.state = ModelLibrary.missing
        let visible = ModelLibrary.filtered(sample(), filter)
        XCTAssertNil(ModelLibrary.selected(visible, selection: "parakeet_tdt_v3"), "caché par le filtre")
        let cohere = ModelLibrary.selected(visible, selection: "cohere_transcribe_8bit")
        XCTAssertNotNil(cohere)
        XCTAssertFalse(ModelLibrary.canUse(cohere), "pas encore téléchargé")
        XCTAssertFalse(ModelLibrary.canUse(nil))
        let all = sample()
        XCTAssertTrue(ModelLibrary.canUse(all.first { $0.id == "parakeet_tdt_v3" }))
        XCTAssertTrue(ModelLibrary.canUse(all.first { $0.system }), "la dictée de macOS")
    }

    /// VoiceOver entend de chaque ligne : nom, éditeur, langues, état,
    /// taille, puis si elle écrit.
    func testEachRowSaysNameMakerLanguagesStateAndSize() {
        let rows = sample(chosen: "parakeet_tdt_v3")
        let cohere = rows.first { $0.id == "cohere_transcribe_8bit" }!
        XCTAssertEqual(ModelLibrary.accessibilityLabel(cohere, .notInstalled),
                       "Cohere Transcribe, Cohere, Français, anglais, Vitesse 15 %, Justesse 96 %, non installé, 600 Mo à télécharger")
        XCTAssertEqual(ModelLibrary.accessibilityLabel(cohere, .downloading(0.25)),
                       "Cohere Transcribe, Cohere, Français, anglais, Vitesse 15 %, Justesse 96 %, téléchargement 25 %")
        var parakeet = rows.first { $0.id == "parakeet_tdt_v3" }!
        parakeet.diskBytes = 2_500_000_000
        XCTAssertEqual(ModelLibrary.accessibilityLabel(parakeet, .installed),
                       "Parakeet v3, NVIDIA, Français, anglais, Vitesse 100 %, Justesse 95 %, installé, 2,5 Go, utilisé")
        let mac = rows.first { $0.system }!
        XCTAssertEqual(ModelLibrary.accessibilityLabel(mac, .system),
                       "\(mac.name), Apple, Français, anglais, sur ce Mac")
        let leftover = model("qwen3_asr_06b_8bit", "Qwen3-ASR 0.6B", disk: 120_000_000, files: true)
        XCTAssertTrue(ModelLibrary.accessibilityLabel(leftover, .failed("réseau"))
            .hasSuffix("échec du téléchargement : réseau, téléchargement incomplet, 120 Mo sur le disque"))
    }
}

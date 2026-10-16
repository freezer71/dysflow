import XCTest
@testable import Sillon

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
}

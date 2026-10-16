import XCTest
@testable import Sillon

final class ModelTableTests: XCTestCase {
    func row(_ id: String = "kokoro_82m_mlx", installed: Bool = false, active: Bool = false,
             bytes: UInt64 = 342_000_000, disk: UInt64 = 0, system: Bool = false) -> VoiceModel {
        VoiceModel(id: id, name: system ? "Voix de macOS" : "Kokoro 82M · MLX", bytes: bytes,
                   installed: installed, active: active, recommended: false, summary: "",
                   family: system ? "system" : "kokoro", precision: "bf16", languagesLabel: "Français, anglais",
                   languages: ["fr", "en"], pace: "", voicesLabel: "", licenses: "",
                   kind: "speech", diskBytes: disk, system: system)
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

    /// VoiceOver dit de chaque ligne son nom, si elle est utilisée, son état
    /// et sa taille.
    func testEachRowSaysItsNameUseStateAndSize() {
        let used = ModelsTable.label(row(installed: true, active: true, disk: 342_000_000), .installed)
        XCTAssertTrue(used.hasPrefix("Kokoro 82M · MLX, utilisé, installé, "), used)
        XCTAssertEqual(ModelsTable.label(row(system: true), .system), "Voix de macOS, non utilisé, sur ce Mac")
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
        XCTAssertFalse(ModelTable.size(row(installed: true, disk: 1_000_000)).isEmpty)
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

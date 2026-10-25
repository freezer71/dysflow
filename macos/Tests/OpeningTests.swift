import UniformTypeIdentifiers
import XCTest
@testable import Dysflow

/// Glisser-déposer un fichier sur le lecteur.
@MainActor
final class OpeningTests: XCTestCase {
    func testOnlyFilesAreOpenedAndTheLastOneWins() {
        let a = URL(fileURLWithPath: "/tmp/a.md")
        let b = URL(fileURLWithPath: "/tmp/b.pdf")
        let web = URL(string: "https://exemple.fr/page")!
        XCTAssertEqual(DroppedFiles.file(from: [a, b]), b)
        XCTAssertEqual(DroppedFiles.file(from: [a, web]), a)
        XCTAssertNil(DroppedFiles.file(from: [web]))
        XCTAssertNil(DroppedFiles.file(from: []))
    }

    func testADroppedFileGoesToTheApp() {
        var opened: [URL] = []
        let name = "dysflow.tests.affichage.ouverture"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        let c = PanelController(model: ReaderModel(reader: DysflowReader.silent()),
                                store: DisplayModeStore(defaults: defaults),
                                openFile: { opened.append($0) })
        let file = URL(fileURLWithPath: "/tmp/cours.md")
        XCTAssertTrue(c.openDropped([file]))
        XCTAssertFalse(c.openDropped([URL(string: "https://exemple.fr")!]))
        XCTAssertEqual(opened, [file])
    }

    /// Le panneau d'ouverture propose Markdown, texte, PDF et images.
    func testTheOpenPanelOffersTextPdfAndImages() {
        XCTAssertTrue(OpenableFiles.types.contains(.pdf))
        XCTAssertTrue(OpenableFiles.types.contains(.image))
        XCTAssertTrue(OpenableFiles.types.contains(.plainText))
    }

    /// Markdown est toujours proposé, même sur une machine sans application
    /// qui le déclare : `.md` et `.markdown` reposent sur le texte brut.
    func testMarkdownIsAlwaysOffered() {
        for ext in ["md", "markdown"] {
            let type = OpenableFiles.markdown(ext)
            XCTAssertTrue(type.conforms(to: .plainText), ext)
            XCTAssertTrue(OpenableFiles.types.contains(type), ext)
        }
    }
}

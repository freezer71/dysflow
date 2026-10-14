import XCTest
@testable import Sillon

@MainActor
final class NotesStoreTests: XCTestCase {
    private var directory: URL!

    override func setUp() {
        super.setUp()
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("sillon-notes-tests-\(UUID().uuidString)")
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: directory)
        super.tearDown()
    }

    func testAPageIsWrittenAndReadBack() {
        let store = NotesStore(directory: directory)
        let page = store.create()
        store.update(page.id, title: "Courses", body: "Du pain.\nDu lait.")
        store.flush()

        let reloaded = NotesStore(directory: directory)
        XCTAssertEqual(reloaded.pages.count, 1)
        XCTAssertEqual(reloaded.pages.first?.title, "Courses")
        XCTAssertEqual(reloaded.pages.first?.body, "Du pain.\nDu lait.")
    }

    func testTheWriteWaitsForAPause() async {
        let store = NotesStore(directory: directory)
        store.writeDelay = .milliseconds(50)
        let page = store.create()
        store.update(page.id, body: "Bonjour")
        let file = directory.appendingPathComponent(page.id + ".md")
        let written = await eventuallyTrue {
            (try? String(contentsOf: file, encoding: .utf8))?.contains("Bonjour") == true
        }
        XCTAssertTrue(written)
    }

    func testTheLastEditedPageComesFirst() {
        var clock = Date(timeIntervalSince1970: 1000)
        let store = NotesStore(directory: directory, now: { clock })
        let first = store.create(title: "Un")
        clock += 10
        let second = store.create(title: "Deux")
        XCTAssertEqual(store.pages.map(\.id), [second.id, first.id])
        clock += 10
        store.update(first.id, body: "modifiée")
        XCTAssertEqual(store.pages.map(\.id), [first.id, second.id])
    }

    func testDeletingRemovesTheFile() {
        let store = NotesStore(directory: directory)
        let page = store.create(title: "À jeter")
        store.delete(page.id)
        XCTAssertTrue(store.pages.isEmpty)
        XCTAssertTrue(NotesStore(directory: directory).pages.isEmpty)
    }

    func testTheFileFormat() {
        XCTAssertEqual(NotesStore.join(title: "Titre", body: "Texte"), "# Titre\n\nTexte")
        XCTAssertEqual(NotesStore.split("# Titre\n\nTexte\n\nSuite").title, "Titre")
        XCTAssertEqual(NotesStore.split("# Titre\n\nTexte\n\nSuite").body, "Texte\n\nSuite")
        // Un fichier écrit à la main, sans titre : tout est corps.
        XCTAssertEqual(NotesStore.split("Juste du texte").title, "")
        XCTAssertEqual(NotesStore.split("Juste du texte").body, "Juste du texte")
    }

    func testWhatTheReaderReceives() {
        let page = NotePage(id: "a", title: "Titre", body: "Texte.", modified: .now)
        XCTAssertEqual(page.markdown, "# Titre\n\nTexte.")
        let untitled = NotePage(id: "b", title: "  ", body: "Texte.", modified: .now)
        XCTAssertEqual(untitled.markdown, "Texte.")
        XCTAssertEqual(untitled.displayTitle, "Sans titre")
    }

    func testALaunchRightAfterBootCountsAsLogin() {
        XCTAssertTrue(LaunchKind.launchedAtLogin(event: nil, uptime: 30))
        XCTAssertFalse(LaunchKind.launchedAtLogin(event: nil, uptime: 3600))
    }

    func testALaunchToOpenFilesIsRecognised() {
        let open = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenDocuments,
                                          targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                          transactionID: AETransactionID(kAnyTransactionID))
        let plain = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenApplication,
                                           targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID),
                                           transactionID: AETransactionID(kAnyTransactionID))
        XCTAssertTrue(LaunchKind.launchedWithFiles(event: open))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: plain))
        XCTAssertFalse(LaunchKind.launchedWithFiles(event: nil))
    }

    /// Fermer les notes quitte sillon, sauf en pleine lecture.
    func testClosingTheNotesQuitsUnlessReading() {
        XCTAssertTrue(AppDelegate.quitsWhenNotesClose(playing: false))
        XCTAssertFalse(AppDelegate.quitsWhenNotesClose(playing: true))
    }
}

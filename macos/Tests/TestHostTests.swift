import XCTest
@testable import Sillon

/// L'hôte des tests unitaires ne touche jamais aux données de l'utilisateur.
@MainActor
final class TestHostTests: XCTestCase {
    func testTheReaderIsSilentUnderXCTestAndUITests() {
        XCTAssertTrue(AppDelegate.usesSilentReader(arguments: [],
                                                   environment: ["XCTestConfigurationFilePath": "/tmp/x"]))
        XCTAssertTrue(AppDelegate.usesSilentReader(arguments: ["--ui-testing"], environment: [:]))
        XCTAssertFalse(AppDelegate.usesSilentReader(arguments: [], environment: [:]))
    }

    func testThisTestHostRunsTheSilentReader() {
        XCTAssertEqual(AppDelegate.shared?.readerIsSilent, true)
    }
}

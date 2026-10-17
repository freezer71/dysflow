import XCTest

@MainActor
extension XCUIElement {
    /// Choisit une section dans la liste de gauche de la fenêtre des
    /// réglages (`section-voix`, `section-dictee`…), puis attend son volet.
    func selectSettingsSection(_ section: String, file: StaticString = #filePath, line: UInt = #line) {
        let row = descendants(matching: .any)["section-\(section)"].firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 5), "ligne section-\(section) absente", file: file, line: line)
        row.click()
        XCTAssertTrue(descendants(matching: .any)["volet-\(section)"].waitForExistence(timeout: 3),
                      "volet \(section) absent", file: file, line: line)
    }
}

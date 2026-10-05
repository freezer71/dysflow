import XCTest

extension XCUIApplication {
    /// L'audit d'accessibilité de macOS, sans la Touch Bar virtuelle.
    ///
    /// Pendant un test d'interface, macOS expose pour toute application au
    /// premier plan une Touch Bar (`AXFunctionRowTopLevelElement`, 685 × 30
    /// en 80, 0), même vide et même sans Touch Bar physique, et ne lui donne
    /// aucun nom : l'audit la dit « sans description » dans toute app,
    /// Calculette comprise (vérifié le 2026-10-05, macOS 26.6). Aucune API
    /// publique ne la nomme. Seul ce cas est écarté ; tout autre problème,
    /// Touch Bar comprise si elle portait des éléments, fait échouer le test.
    func performAccessibilityAuditOutsideTouchBar() throws {
        try performAccessibilityAudit { issue in
            guard let element = issue.element, element.elementType == .touchBar else { return false }
            return element.children(matching: .any).count == 0
        }
    }
}

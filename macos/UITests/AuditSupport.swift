import XCTest

@MainActor
extension XCUIApplication {
    /// L'audit d'accessibilité de macOS, sans ce que le système ajoute lui-même.
    ///
    /// Pendant un test d'interface, macOS expose pour toute application au
    /// premier plan une Touch Bar (`AXFunctionRowTopLevelElement`, 685 × 30
    /// en 80, 0), même sans Touch Bar physique, et ne lui donne aucun nom :
    /// l'audit la dit « sans description » dans toute app, Calculette comprise
    /// (vérifié le 2026-10-05, macOS 26.6). Quand un champ de texte a le focus,
    /// le système y met ses propres boutons (« emoji & symbols », suggestions
    /// de saisie), eux aussi sans nom ni action pour l'audit (vu le
    /// 2026-10-08). Sillage ne définit aucune Touch Bar : tout ce qui s'y trouve
    /// vient du système et est écarté.
    ///
    /// Avec `visibleIn`, un défaut de contraste n'est retenu que pour un
    /// élément entièrement dans la partie visible de cette fenêtre : un libellé
    /// que le défilement cache sous le bord ou sous la barre d'outils n'est pas
    /// dessiné là, l'audit y mesure d'autres pixels.
    /// Il faut alors faire défiler et auditer de nouveau pour le couvrir.
    func performAccessibilityAuditOutsideTouchBar(visibleIn window: XCUIElement? = nil) throws {
        let touchBar = touchBars.firstMatch
        let touchBarFrame = touchBar.exists ? touchBar.frame : .null
        // La partie visible du contenu : la fenêtre, sous sa barre d'outils
        // (le contenu défile dessous, voilé).
        let visibleFrame: CGRect? = window.map { window in
            let frame = window.frame
            let toolbar = window.toolbars.firstMatch
            guard toolbar.exists else { return frame }
            let top = toolbar.frame.maxY
            return CGRect(x: frame.minX, y: top, width: frame.width, height: frame.maxY - top)
        }
        try performAccessibilityAudit { issue in
            // Le détail de chaque problème, que le message d'échec ne donne pas.
            print("AUDIT \(issue.compactDescription) — \(issue.detailedDescription) — \(issue.element.map { "\($0.elementType.rawValue) « \($0.label) » id=\($0.identifier) \($0.frame)" } ?? "sans élément")")
            guard let element = issue.element else {
                // Un lien parent / enfant que l'audit ne peut rattacher à aucun
                // élément : les boutons que le système met dans la Touch Bar
                // pendant une saisie. Hors saisie, la Touch Bar est vide et ce
                // cas fait échouer le test.
                return issue.auditType == .parentChild && touchBar.children(matching: .any).count > 0
            }
            if element.elementType == .touchBar || touchBarFrame.intersects(element.frame) { return true }
            if issue.auditType == .contrast, let visibleFrame, !visibleFrame.contains(element.frame) { return true }
            return false
        }
    }
}

extension XCUIElement {
    func waitForLabel(_ label: String, timeout: TimeInterval = 2) -> Bool {
        let predicate = NSPredicate(format: "label == %@", label)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: self)
        return XCTWaiter.wait(for: [exp], timeout: timeout) == .completed
    }
}

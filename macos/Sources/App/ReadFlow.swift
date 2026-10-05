/// Ce que fait le raccourci « Lire », dans l'ordre de la spec : la voix se
/// charge pendant que le texte se capture ; la capture se fait avant que le
/// panneau ne prenne le clavier, sinon ⌘C le viserait lui.
@MainActor
struct ReadFlow {
    let prepare: () -> Void
    let capture: () async -> CaptureOutcome
    let open: (String) -> Void
    /// Une copie de gestionnaire de mots de passe n'est jamais lue.
    let clipboardIsConcealed: () -> Bool
    /// Hors de l'acteur principal (l'OCR d'une image peut durer) : attendu.
    let openClipboard: () async throws -> Void
    let say: (String) -> Void
    let show: () -> Void

    func run() async {
        prepare()
        switch await capture() {
        case .text(let text):
            open(text)
        case .veto(.secure):
            say("Champ sécurisé : rien n'est lu")
        case .veto(.ownApp):
            say("sillon est au premier plan : sélectionnez dans une autre application")
        case .noSelection, .unsupported:
            await readClipboard()
        }
        show()
    }

    private func readClipboard() async {
        guard !clipboardIsConcealed() else {
            say("Presse-papiers protégé : rien n'est lu")
            return
        }
        do { try await openClipboard() } catch { say("Sélectionnez ou copiez du texte") }
    }
}

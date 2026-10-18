import Foundation

/// Ce que fait le raccourci « Lire », dans l'ordre de la spec : la voix se
/// charge pendant que le texte se capture ; la capture se fait avant que le
/// panneau ne prenne le clavier, sinon ⌘C le viserait lui.
@MainActor
struct ReadFlow {
    let prepare: () -> Void
    let capture: () async -> CaptureOutcome
    /// Le texte et sa source (« sélection », « presse-papiers »).
    let open: (_ text: String, _ source: String) -> Void
    /// Le presse-papiers vérifié et lu d'un seul coup (voir `ClipboardSnapshot`).
    let readClipboard: () -> ClipboardRead
    /// Une image passe au moteur avec ses octets vérifiés, hors de l'acteur
    /// principal (l'OCR peut durer).
    let openImage: (Data) async throws -> Void
    let say: (String) -> Void
    let show: () -> Void

    func run() async {
        prepare()
        switch await capture() {
        case .text(let text):
            open(text, "sélection")
        case .veto(.secure):
            say("Champ sécurisé : rien n'est lu")
        case .veto(.ownApp):
            say("sillage est au premier plan : sélectionnez dans une autre application")
        case .noSelection, .unsupported:
            await openClipboard()
        }
        show()
    }

    private func openClipboard() async {
        switch readClipboard() {
        case .text(let text):
            open(text, "presse-papiers")
        case .image(let data):
            do { try await openImage(data) } catch { say("Sélectionnez ou copiez du texte") }
        case .concealed:
            say("Presse-papiers protégé : rien n'est lu")
        case .empty:
            say("Sélectionnez ou copiez du texte")
        }
    }
}

/// Une seule lecture à la fois : deux captures simultanées se marcheraient
/// dessus (la seconde sauvegarderait la copie de la première comme
/// presse-papiers de l'utilisateur).
@MainActor
final class ReadGate {
    private(set) var busy = false

    /// Faux si une lecture est déjà en cours : l'appui est ignoré.
    @discardableResult
    func run(_ body: () async -> Void) async -> Bool {
        guard !busy else { return false }
        busy = true
        defer { busy = false }
        await body()
        return true
    }
}

import Foundation
import Observation

/// L'état du lecteur, relu à chaque image affichée.
///
/// Swift ne garde rien de propre : l'instantané vient de Rust, le document
/// n'est relu que lorsque sa révision change.
@MainActor
@Observable
final class ReaderModel {
    @ObservationIgnored private let reader: SillonReader
    @ObservationIgnored private var ticker: DisplayTicker?

    private(set) var snapshot: Snapshot
    private(set) var document: DocView
    var message: String?

    init(reader: SillonReader) {
        self.reader = reader
        snapshot = reader.snapshot()
        document = reader.document()
    }

    /// Vrai quand le panneau est affiché : le suivi à chaque image est alors utile.
    var isDisplayed = false {
        didSet {
            if isDisplayed { refresh() }
            updateTicking()
        }
    }

    /// Le suivi à la cadence de l'écran tourne-t-il ?
    var isTicking: Bool { ticker?.isRunning ?? false }

    /// Le suivi ne tourne que si le panneau est affiché ou si la lecture avance :
    /// au repos, une app de barre des menus ne coûte rien.
    func start() {
        if ticker == nil {
            ticker = DisplayTicker { [weak self] in self?.refresh() }
        }
        ticker?.start()
    }

    func stop() {
        ticker?.stop()
    }

    private func updateTicking() {
        if isDisplayed || snapshot.playing {
            start()
        } else {
            stop()
        }
    }

    /// Relance le suivi et relit tout de suite l'état.
    private func wake() {
        start()
        refresh()
    }

    func refresh() {
        let next = reader.snapshot()
        if next.revision != snapshot.revision {
            document = reader.document()
        }
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if next != snapshot {
            snapshot = next
        }
        updateTicking()
    }

    func send(_ command: ReaderCommand) {
        reader.command(command: command)
        wake()
    }

    func open(text: String, source: String) {
        message = nil
        reader.openText(markdown: text, source: source)
        wake()
    }

    /// Le lecteur lit le presse-papiers sur le fil appelant et peut passer
    /// une image à l'OCR (plusieurs secondes) : hors du fil principal.
    func openClipboard() async throws {
        message = nil
        let reader = reader
        try await Task.detached { try reader.openClipboard() }.value
        wake()
    }

    func prepareVoice() {
        reader.prepareVoice()
        wake()
    }
    func restVoice() { reader.restVoice() }

    // Ces appels passent par le fil du lecteur et bloquent jusqu'à 5 s :
    // pas de long travail sur le fil principal.
    func bookmarks() -> [BookmarkView] { reader.bookmarks() }
    func settings() -> SettingsView { reader.settings() }
    func updateSettings(_ s: SettingsView) throws { try reader.updateSettings(settings: s) }
    func voices() -> [VoiceModel] { reader.voices() }
    func installVoice(_ id: String) throws { try reader.installVoice(id: id) }
    func useVoice(_ id: String) throws { try reader.useVoice(id: id) }

    /// La phrase en cours telle qu'elle est écrite.
    var currentSentenceText: String {
        for block in document.blocks {
            if let s = block.sentences.first(where: { $0.id == snapshot.sentence }) {
                return s.words.map(\.text).joined(separator: " ")
            }
        }
        return ""
    }
}

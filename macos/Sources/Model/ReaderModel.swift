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

    func start() {
        if ticker == nil {
            ticker = DisplayTicker { [weak self] in self?.refresh() }
        }
        ticker?.start()
    }

    func stop() {
        ticker?.stop()
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
    }

    func send(_ command: ReaderCommand) {
        reader.command(command: command)
    }

    func open(text: String, source: String) {
        message = nil
        reader.openText(markdown: text, source: source)
    }

    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func openClipboard() throws {
        message = nil
        try reader.openClipboard()
    }

    func prepareVoice() { reader.prepareVoice() }
    func restVoice() { reader.restVoice() }

    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func bookmarks() -> [BookmarkView] { reader.bookmarks() }
    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func settings() -> SettingsView { reader.settings() }
    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func updateSettings(_ s: SettingsView) throws { try reader.updateSettings(settings: s) }
    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func voices() -> [VoiceModel] { reader.voices() }
    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
    func installVoice(_ id: String) throws { try reader.installVoice(id: id) }
    /// Bloque jusqu'à 5 s (fil du lecteur) : pas de long travail sur le fil principal.
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

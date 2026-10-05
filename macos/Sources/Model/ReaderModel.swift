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
    @ObservationIgnored private let now: () -> Date
    /// Après une commande, l'effet n'arrive que quelques ms plus tard : le suivi
    /// ne peut pas s'arrêter avant cette échéance.
    @ObservationIgnored private var keepAliveUntil = Date.distantPast
    static let gracePeriod: TimeInterval = 0.5

    private(set) var snapshot: Snapshot
    private(set) var document: DocView
    var message: String?

    init(reader: SillonReader, now: @escaping () -> Date = Date.init) {
        self.reader = reader
        self.now = now
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
        if isDisplayed || snapshot.playing || now() < keepAliveUntil {
            start()
        } else {
            stop()
        }
    }

    /// Relance le suivi et relit tout de suite l'état.
    private func wake() {
        keepAliveUntil = now().addingTimeInterval(Self.gracePeriod)
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

    /// Compte les commandes envoyées : la vue de lecture reprend le suivi à
    /// chacune, même si l'utilisateur faisait défiler le texte.
    private(set) var commandCount = 0

    func send(_ command: ReaderCommand) {
        commandCount &+= 1
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
    /// Les signets, lus hors du fil principal.
    func loadBookmarks() async -> [BookmarkView] {
        let reader = reader
        return await Task.detached { reader.bookmarks() }.value
    }
    /// Les réglages, lus hors du fil principal.
    func loadSettings() async -> SettingsView {
        let reader = reader
        return await Task.detached { reader.settings() }.value
    }
    /// Relit les réglages, applique `change`, les écrit : hors du fil principal.
    func changeSettings(_ change: @escaping @Sendable (inout SettingsView) -> Void) async throws {
        let reader = reader
        try await Task.detached {
            var s = reader.settings()
            change(&s)
            try reader.updateSettings(settings: s)
        }.value
    }
    /// Les voix du catalogue, lues hors du fil principal.
    func loadVoices() async -> [VoiceModel] {
        let reader = reader
        return await Task.detached { reader.voices() }.value
    }
    func installVoiceAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.installVoice(id: id) }.value
        wake()
    }
    func useVoiceAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.useVoice(id: id) }.value
    }
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

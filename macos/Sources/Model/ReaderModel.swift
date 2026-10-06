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
    @ObservationIgnored private let frames: FrameSource
    @ObservationIgnored private let now: () -> Date
    /// Après une commande, l'effet n'arrive que quelques ms plus tard : le suivi
    /// ne peut pas s'arrêter avant cette échéance.
    @ObservationIgnored private var keepAliveUntil = Date.distantPast
    static let gracePeriod: TimeInterval = 0.5

    private(set) var snapshot: Snapshot
    private(set) var document: DocView {
        didSet { outline() }
    }
    var message: String?
    /// Le sommaire : les sections qui ont une phrase. Calculé une fois par
    /// révision, pas à chaque évaluation de la vue.
    private(set) var tableOfContents: [SectionView] = []
    /// Les repères de section sur la ligne de temps, une fois par révision.
    private(set) var sectionPositions: [Double] = []
    /// Le fil d'Ariane de la section lue, recalculé quand elle change.
    private(set) var breadcrumb = ""

    init(reader: SillonReader, now: @escaping () -> Date = Date.init,
         frames: FrameSource? = nil) {
        self.reader = reader
        self.now = now
        self.frames = frames ?? DisplayLinkFrames()
        snapshot = Self.published(reader.snapshot())
        document = reader.document()
        outline()
        self.frames.onFrame = { [weak self] in self?.refresh() }
    }

    private func outline() {
        tableOfContents = SectionMarkers.tableOfContents(document)
        sectionPositions = SectionMarkers.positions(document)
        breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
    }

    /// Vrai quand le panneau est affiché.
    var isDisplayed = false {
        didSet {
            if isDisplayed { refresh() }
            updateTicking()
        }
    }

    /// Le suivi à la cadence de l'écran tourne-t-il ?
    var isTicking: Bool { frames.isRunning }

    /// Le rythme auquel l'état est relu.
    enum Cadence: Equatable {
        /// Rien : panneau masqué, rien en cours.
        case none
        /// Un coup d'œil lent : panneau affiché en pause, pour qu'un message
        /// s'efface ou que l'état de la voix change à l'écran.
        case glance
        /// À chaque image : la lecture avance, une commande vient de partir,
        /// la voix se charge ou un téléchargement progresse.
        case screen
    }
    @ObservationIgnored private(set) var cadence: Cadence = .none
    @ObservationIgnored private var glanceTimer: Timer?
    static let glanceInterval: TimeInterval = 0.25

    /// Un téléchargement est en cours (un échec resté là ne compte pas).
    static func isDownloading(_ d: DownloadView?) -> Bool { d.map { $0.failed == nil } ?? false }

    /// La décision de suivi, sans état : testable. Masqué, une commande (la
    /// grâce) ou une lecture se voient quand même ; affiché en pause, un coup
    /// d'œil lent suffit — le panneau flotte, il peut rester visible des heures.
    static func cadence(displayed: Bool, playing: Bool, grace: Bool, loading: Bool,
                        download: DownloadView?) -> Cadence {
        if playing || grace || loading || isDownloading(download) { return .screen }
        return displayed ? .glance : .none
    }

    private func updateTicking() {
        let loading: Bool
        if case .loading = snapshot.voice { loading = true } else { loading = false }
        let next = Self.cadence(displayed: isDisplayed, playing: snapshot.playing,
                                grace: now() < keepAliveUntil, loading: loading,
                                download: snapshot.download)
        cadence = next
        if next == .screen { start() } else { stop() }
        if next == .glance { startGlance() } else { stopGlance() }
    }

    /// Le suivi à la cadence de l'écran.
    func start() {
        frames.start()
    }

    func stop() {
        frames.stop()
    }

    private func startGlance() {
        guard glanceTimer == nil else { return }
        let timer = Timer(timeInterval: Self.glanceInterval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        RunLoop.main.add(timer, forMode: .common)
        glanceTimer = timer
    }

    private func stopGlance() {
        glanceTimer?.invalidate()
        glanceTimer = nil
    }

    /// L'instantané tel que l'écran le montre : temps à la seconde,
    /// avancement au millième. Le reste de ce que Rust donne à chaque image
    /// ne republie rien, et SwiftUI ne réévalue pas toute l'interface. La
    /// part du mot dite n'en fait pas partie : le trait sous la ligne la
    /// relit avec son propre lien d'affichage (voir `liveSnapshot`).
    static func published(_ s: Snapshot) -> Snapshot {
        var s = s
        s.wordProgress = 0
        s.elapsed = s.elapsed.map { $0.isFinite ? $0.rounded(.down) : $0 }
        s.total = s.total.map { $0.isFinite ? $0.rounded() : $0 }
        s.progress = s.progress.isFinite ? (s.progress * 1000).rounded() / 1000 : s.progress
        return s
    }

    /// L'instantané tel que Rust le donne à l'instant, sans rien publier :
    /// pour ce qui change à chaque image et que SwiftUI ne doit pas voir.
    func liveSnapshot() -> Snapshot {
        reader.snapshot()
    }

    /// Relance le suivi et relit tout de suite l'état.
    private func wake() {
        keepAliveUntil = now().addingTimeInterval(Self.gracePeriod)
        start()
        refresh()
    }

    func refresh() {
        let next = Self.published(reader.snapshot())
        let newDocument = next.revision != snapshot.revision
        let newSection = next.section != snapshot.section
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if next != snapshot {
            snapshot = next
        }
        if newDocument {
            document = reader.document()   // recalcule sommaire, repères et fil
        } else if newSection {
            breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
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

    /// Une image copiée, déjà vérifiée : le moteur la passe à l'OCR
    /// (plusieurs secondes), hors du fil principal.
    func openImage(_ data: Data) async throws {
        message = nil
        let reader = reader
        try await Task.detached { try reader.openImage(bytes: data) }.value
        wake()
    }

    func prepareVoice() {
        reader.prepareVoice()
        wake()
    }
    func restVoice() { reader.restVoice() }

    // Ces appels passent par le fil du lecteur et bloquent jusqu'à 5 s :
    // pas de long travail sur le fil principal.
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
    /// Change à chaque réglage écrit : le panneau relit son affichage.
    private(set) var settingsRevision = 0

    /// Applique un patch et l'écrit, hors du fil principal ; rend les
    /// valeurs telles qu'appliquées (bornes comprises).
    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView {
        let reader = reader
        let applied = try await Task.detached { try reader.updateSettings(patch: patch) }.value
        settingsRevision &+= 1
        wake()
        return applied
    }
    /// Voix, langues et réglages honorés par le modèle actif.
    func loadCapabilities() async -> ModelCapabilities {
        let reader = reader
        return await Task.detached { reader.capabilities() }.value
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
    /// Change de modèle : la voix se recharge, le suivi montre le chargement.
    func useVoiceAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.useVoice(id: id) }.value
        wake()
    }
    func cancelDownloadAsync() async throws {
        let reader = reader
        try await Task.detached { try reader.cancelDownload() }.value
        wake()
    }
    func removeModelAsync(_ id: String) async throws {
        let reader = reader
        try await Task.detached { try reader.removeModel(id: id) }.value
    }
    func freeSpace() async -> UInt64? {
        let reader = reader
        return await Task.detached { reader.freeSpace() }.value
    }
    /// « Écouter » : la phrase d'exemple avec les réglages en cours.
    func previewVoice() async throws {
        let reader = reader
        try await Task.detached { try reader.previewVoice() }.value
        wake()
    }
    func stopPreview() async {
        let reader = reader
        await Task.detached { reader.stopPreview() }.value
        wake()
    }
    /// Vide le cache des documents ; rend les octets libérés.
    func clearDocumentCache() async throws -> UInt64 {
        let reader = reader
        return try await Task.detached { try reader.clearDocumentCache() }.value
    }
    /// Tous les signets, tous documents confondus.
    func loadAllBookmarks() async -> [StoredBookmarkView] {
        let reader = reader
        return await Task.detached { reader.allBookmarks() }.value
    }
    func removeBookmarkAsync(document: String, sentence: UInt32) async throws {
        let reader = reader
        try await Task.detached { try reader.removeBookmark(document: document, sentence: sentence) }.value
    }
    /// Le dossier de `config.toml` (lecture directe, sans le fil du lecteur).
    var configDirectory: String? { reader.configDir() }

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

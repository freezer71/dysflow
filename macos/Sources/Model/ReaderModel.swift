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
    /// Prévenu quand l'instantané publié change (à la seconde, au millième
    /// d'avancement, à l'état) — jamais à chaque image : « À l'écoute » suit
    /// l'état sans minuterie à lui.
    @ObservationIgnored var onChange: (@MainActor (Snapshot, DocView) -> Void)?
    /// Prévenu quand la voix active a pu changer : voix choisie, proposition
    /// acceptée, téléchargement fini (le passage à Kokoro accepté attend sa
    /// fin). L'app y prépare les noyaux Metal de Kokoro.
    @ObservationIgnored var onVoiceMayHaveChanged: (@MainActor () -> Void)?
    /// La préparation des noyaux Metal de Kokoro (`KokoroWarmUp`), observée
    /// par l'onglet Voix.
    @ObservationIgnored let voicePreparation = VoicePreparation()
    /// Tests d'interface seulement (`UITestStaging`) : ce qui est posé
    /// sur chaque instantané publié.
    @ObservationIgnored var staged: ((inout Snapshot) -> Void)?
    /// Tests d'interface seulement (`UITestStaging.kokoro`) : les capacités
    /// de Kokoro, pour montrer ses voix sous le lecteur muet.
    @ObservationIgnored var stagedCapabilities: ModelCapabilities?

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

    /// La boucle principale retient les minuteries : elles partent avec le
    /// modèle.
    deinit {
        pollTimer?.invalidate()
        graceTimer?.invalidate()
    }

    private func outline() {
        tableOfContents = SectionMarkers.tableOfContents(document)
        sectionPositions = SectionMarkers.positions(document)
        breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
    }

    /// Ce que la fenêtre montre de l'état : rien (arrière-plan), la barre
    /// du mode petit, ou le texte (moyen, grand).
    enum Presence: Equatable { case hidden, bar, full }

    /// Posé par le panneau à chaque mode : il règle la cadence de relecture.
    var presence: Presence = .hidden {
        didSet {
            guard presence != oldValue else { return }
            if presence != .hidden { refresh() }
            updateTicking()
        }
    }

    /// Le suivi à la cadence de l'écran tourne-t-il ?
    var isTicking: Bool { frames.isRunning }

    /// Le rythme auquel l'état est relu.
    enum Cadence: Equatable {
        /// Rien : en pause, sans texte affiché.
        case none
        /// Une fois par seconde : la lecture avance en arrière-plan ; seuls
        /// la phrase et le temps à la seconde comptent (« À l'écoute »).
        case second
        /// Un coup d'œil, 4 fois par seconde : texte affiché en pause (un
        /// message s'efface, la voix change d'état), ou barre du mode petit
        /// pendant la lecture (temps à la seconde, avancement au millième).
        case glance
        /// À chaque image : le texte est affiché et la lecture avance, une
        /// commande vient de partir, la voix se charge ou un téléchargement
        /// progresse.
        case screen
    }
    @ObservationIgnored private(set) var cadence: Cadence = .none
    @ObservationIgnored private var pollTimer: Timer?
    /// La fin de la grâce d'une commande (voir `scheduleGraceEnd`).
    @ObservationIgnored private var graceTimer: Timer?
    static let glanceInterval: TimeInterval = 0.25
    static let secondInterval: TimeInterval = 1

    /// Un téléchargement est en cours (un échec resté là ne compte pas).
    static func isDownloading(_ d: DownloadView?) -> Bool { d.map { $0.failed == nil } ?? false }
    /// Un téléchargement en cours vient de finir (réussi ou non).
    static func downloadEnded(from old: DownloadView?, to new: DownloadView?) -> Bool {
        isDownloading(old) && !isDownloading(new)
    }

    /// La décision de suivi, sans état : testable. Quelque chose avance
    /// (lecture, commande récente, voix qui se charge, téléchargement) : à
    /// chaque image si le texte est affiché, au coup d'œil pour la barre,
    /// à la seconde en arrière-plan. En pause, seul le texte affiché garde un
    /// coup d'œil — le panneau flotte, il peut rester visible des heures.
    static func cadence(presence: Presence, playing: Bool, grace: Bool, loading: Bool,
                        download: DownloadView?) -> Cadence {
        if playing || grace || loading || isDownloading(download) {
            switch presence {
            case .full: return .screen
            case .bar: return .glance
            case .hidden: return .second
            }
        }
        return presence == .full ? .glance : .none
    }

    private func updateTicking() {
        let loading: Bool
        if case .loading = snapshot.voice { loading = true } else { loading = false }
        let next = Self.cadence(presence: presence, playing: snapshot.playing,
                                grace: now() < keepAliveUntil, loading: loading,
                                download: snapshot.download)
        cadence = next
        if next == .screen { start() } else { stop() }
        if let interval = Self.pollInterval(for: next) { startPolling(every: interval) } else { stopPolling() }
    }

    /// Le suivi à la cadence de l'écran.
    func start() {
        frames.start()
    }

    func stop() {
        frames.stop()
    }

    /// L'intervalle de la minuterie de relecture pour une cadence. À la
    /// cadence de l'écran, la seconde reste en filet : un écran en veille
    /// n'envoie plus d'images, et l'état doit pourtant être relu.
    static func pollInterval(for cadence: Cadence) -> TimeInterval? {
        switch cadence {
        case .none: return nil
        case .glance: return glanceInterval
        case .second, .screen: return secondInterval
        }
    }

    /// Une seule minuterie de relecture, remplacée si le rythme change.
    private func startPolling(every interval: TimeInterval) {
        if let pollTimer, pollTimer.timeInterval == interval { return }
        stopPolling()
        let timer = Timer(timeInterval: interval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
        // Le système peut regrouper les réveils : une marge d'un cinquième.
        timer.tolerance = interval * 0.2
        RunLoop.main.add(timer, forMode: .common)
        pollTimer = timer
    }

    private func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
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
        scheduleGraceEnd()
        // La cadence (écran, coup d'œil ou seconde) se décide selon la
        // présence de la fenêtre.
        refresh()
    }

    /// La fin de la grâce, à l'heure : sans image (écran en veille), rien
    /// d'autre ne la verrait, et la cadence de l'écran resterait. Une seule
    /// minuterie, sans répétition, remplacée à chaque commande.
    private func scheduleGraceEnd() {
        graceTimer?.invalidate()
        let timer = Timer(timeInterval: Self.gracePeriod + 0.01, repeats: false) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.graceTimer = nil
                self?.refresh()
            }
        }
        timer.tolerance = 0.05
        RunLoop.main.add(timer, forMode: .common)
        graceTimer = timer
    }

    func refresh() {
        var next = Self.published(reader.snapshot())
        staged?(&next)
        let changed = next != snapshot
        let newDocument = next.revision != snapshot.revision
        let newSection = next.section != snapshot.section
        let downloadEnded = Self.downloadEnded(from: snapshot.download, to: next.download)
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if changed {
            snapshot = next
        }
        if newDocument {
            document = reader.document()   // recalcule sommaire, repères et fil
        } else if newSection {
            breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
        }
        if changed { onChange?(snapshot, document) }
        if downloadEnded { onVoiceMayHaveChanged?() }
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
    /// Le repos se voit tout de suite (grâce), même fenêtre masquée.
    func restVoice() {
        reader.restVoice()
        wake()
    }

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
        if let stagedCapabilities { return stagedCapabilities }
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
        onVoiceMayHaveChanged?()
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
    func stopPreview() async {
        let reader = reader
        await Task.detached { reader.stopPreview() }.value
        wake()
    }
    /// La langue du document gardé (`nil` : automatique), hors du fil principal.
    func setDocumentLanguage(_ language: String?) async throws {
        let reader = reader
        try await Task.detached { try reader.setDocumentLanguage(language: language) }.value
        wake()
    }
    /// « Écouter » une voix dans sa langue.
    func previewVoice(in language: String) async throws {
        let reader = reader
        try await Task.detached { try reader.previewVoiceIn(language: language) }.value
        wake()
    }
    /// Le passage unique à Kokoro : appelé une fois au lancement. Rend
    /// l'avis à montrer (passage fait, ou proposition dans `snapshot.offer`).
    func migrateVoice() async -> String? {
        let reader = reader
        let notice = await Task.detached { reader.migrateVoice() }.value
        wake()
        return notice
    }
    /// « Passer à Kokoro » : le téléchargement démarre, hors du fil principal.
    func acceptVoiceOffer() async throws {
        let reader = reader
        try await Task.detached { try reader.acceptVoiceOffer() }.value
        wake()
        onVoiceMayHaveChanged?()
    }
    /// « Plus tard » : le refus est retenu.
    /// Le refus s'écrit dans `config.toml` : hors du fil principal.
    func dismissVoiceOffer() async {
        let reader = reader
        await Task.detached { reader.dismissVoiceOffer() }.value
        wake()
    }
    /// « Remplacer » un `config.toml` illisible (une copie est gardée à
    /// côté), hors du fil principal.
    func replaceUnreadableConfig() async throws {
        let reader = reader
        try await Task.detached { try reader.replaceUnreadableConfig() }.value
        wake()
    }
    /// « Compris » : l'appel attend le verrou du lecteur, hors du fil
    /// principal comme les autres réponses à l'offre.
    func dismissNotice() async {
        let reader = reader
        await Task.detached { reader.dismissNotice() }.value
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

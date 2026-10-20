import Foundation
import Observation

/// L'état du lecteur, relu à chaque image affichée.
///
/// Swift ne garde rien de propre : l'instantané vient de Rust, le document
/// n'est relu que lorsque sa révision change. Le rythme de relecture vit
/// dans `RefreshSchedule`, les sorties audio dans `AudioOutputs`, les appels
/// au moteur hors du fil principal dans `ReaderModel+Engine.swift`.
@MainActor
@Observable
final class ReaderModel {
    @ObservationIgnored let reader: SillageReader
    @ObservationIgnored private let schedule: RefreshSchedule
    static let gracePeriod = RefreshSchedule.gracePeriod

    /// L'état publié, sans le mot lu (voir `published`) : la barre, le
    /// sommaire et le mini ne se réévaluent pas à chaque mot.
    private(set) var snapshot: Snapshot
    /// Le mot lu, observé à part : seule la vue de lecture le lit.
    private(set) var word: UInt32?
    /// L'instantané tel que Rust l'a donné à la dernière relecture, part du
    /// mot comprise ; rien ne l'observe.
    @ObservationIgnored private var live: Snapshot
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
    /// Le dernier refus de l'installation automatique de la voix ; nil après
    /// une réussite. Avec « Réessayer » dans la ligne d'état.
    var voiceSetupError: String?

    /// Dit un échec de la voix à VoiceOver : une annonce à chaque échec,
    /// même au texte identique (sans VoiceOver, ou sans fenêtre, rien).
    @ObservationIgnored var announceFailure: @MainActor (String) -> Void = { Announcement.post($0) }

    /// Au lancement et à chaque « Réessayer ». `ensure` : l'appel au moteur
    /// (celui du modèle par défaut), pour les tests.
    func installDefaultVoice(ensure: (@MainActor () async throws -> Void)? = nil) async {
        do {
            if let ensure { try await ensure() } else { try await ensureDefaultVoice() }
            voiceSetupError = nil
        } catch {
            let message = SettingsStore.message(of: error)
            voiceSetupError = message
            announceFailure(VoiceStatus.failed(message).text)
        }
    }

    /// La sortie audio choisie et les sorties branchées.
    @ObservationIgnored let audioOutputs: AudioOutputs

    init(reader: SillageReader, now: @escaping () -> Date = Date.init,
         frames: FrameSource? = nil) {
        self.reader = reader
        schedule = RefreshSchedule(frames: frames ?? DisplayLinkFrames(), now: now)
        audioOutputs = AudioOutputs(list: { reader.audioOutputs() })
        let first = reader.snapshot()
        live = first
        word = first.word
        snapshot = Self.published(first)
        document = reader.document()
        outline()
        schedule.refresh = { [weak self] in self?.refresh() }
    }

    private func outline() {
        tableOfContents = SectionMarkers.tableOfContents(document)
        sectionPositions = SectionMarkers.positions(document)
        breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
    }

    typealias Presence = RefreshSchedule.Presence
    typealias Cadence = RefreshSchedule.Cadence

    /// Posé par le panneau à chaque mode : il règle la cadence de relecture.
    var presence: Presence = .hidden {
        didSet {
            guard presence != oldValue else { return }
            if presence != .hidden { refresh() }
            updateTicking()
        }
    }

    /// Le suivi à la cadence de l'écran tourne-t-il ?
    var isTicking: Bool { schedule.isTicking }
    /// Le rythme de relecture en cours.
    var cadence: Cadence { schedule.cadence }

    /// Un téléchargement est en cours (un échec resté là ne compte pas).
    static func isDownloading(_ d: DownloadView?) -> Bool { d.map { $0.failed == nil } ?? false }
    /// Un téléchargement en cours vient de finir (réussi ou non).
    static func downloadEnded(from old: DownloadView?, to new: DownloadView?) -> Bool {
        isDownloading(old) && !isDownloading(new)
    }

    private func updateTicking() {
        schedule.update(presence: presence, snapshot: snapshot)
    }

    /// Le suivi à la cadence de l'écran.
    func start() {
        schedule.start()
    }

    func stop() {
        schedule.stop()
    }

    /// L'instantané tel que l'écran le montre : temps à la seconde,
    /// avancement au millième. Le reste de ce que Rust donne à chaque image
    /// ne republie rien, et SwiftUI ne réévalue pas toute l'interface. Le
    /// mot lu n'en fait pas partie (il est publié à part, dans `word`), ni
    /// la part du mot dite : le trait sous la ligne la relit avec son propre
    /// lien d'affichage (voir `liveSnapshot`).
    static func published(_ s: Snapshot) -> Snapshot {
        var s = s
        s.word = nil
        s.wordProgress = 0
        s.elapsed = s.elapsed.map { $0.isFinite ? $0.rounded(.down) : $0 }
        s.total = s.total.map { $0.isFinite ? $0.rounded() : $0 }
        s.progress = s.progress.isFinite ? (s.progress * 1000).rounded() / 1000 : s.progress
        return s
    }

    /// L'instantané tel que Rust l'a donné à la dernière relecture, sans
    /// rien publier : pour ce qui change à chaque image et que SwiftUI ne
    /// doit pas voir. Pas de nouvel appel au moteur : la relecture suit déjà
    /// l'écran pendant la lecture.
    func liveSnapshot() -> Snapshot {
        live
    }

    /// Relance le suivi et relit tout de suite l'état.
    func wake() {
        schedule.wake()
        // La cadence (écran, coup d'œil ou seconde) se décide selon la
        // présence de la fenêtre.
        refresh()
    }

    func refresh() {
        let raw = reader.snapshot()
        live = raw
        if raw.word != word { word = raw.word }
        let next = Self.published(raw)
        let changed = next != snapshot
        let newDocument = next.revision != snapshot.revision
        let newSection = next.section != snapshot.section
        let downloadEnded = Self.downloadEnded(from: snapshot.download, to: next.download)
        // Un téléchargement qui échoue se dit une fois (à l'échec, pas à chaque image).
        if let failed = next.download?.failed, failed != snapshot.download?.failed {
            announceFailure(VoiceStatus.failed(failed).text)
        }
        // Ne publier qu'un changement réel : SwiftUI ne redessine pas pour rien.
        if changed {
            snapshot = next
        }
        if newDocument || (pendingResume != nil && !document.complete) {
            document = reader.document()   // recalcule sommaire, repères et fil
        } else if newSection {
            breadcrumb = SectionMarkers.breadcrumb(document, section: snapshot.section)
        }
        if changed { onChange?(snapshot, document) }
        if downloadEnded { onVoiceMayHaveChanged?() }
        if pendingResume != nil { resumeIfReady() }
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
        sourceText = text
        sourceName = source
        pendingResume = nil
        reader.openText(markdown: text, source: source)
        wake()
    }

    /// Le texte ouvert, tel qu'il est arrivé : le mode édition le reprend.
    /// Nil pour une image (l'OCR) : le document est alors réécrit.
    @ObservationIgnored private(set) var sourceText: String?
    @ObservationIgnored private var sourceName = "presse-papiers"

    /// Ce que le mode édition montre : le texte ouvert, sinon le document
    /// réécrit en Markdown.
    var editableText: String {
        sourceText ?? EditableText.markdown(from: document)
    }

    /// Le texte modifié remplace le document. La voix reprend à la même
    /// phrase (ou à la dernière, si le texte a raccourci), et repart si
    /// elle lisait.
    func reopen(edited text: String, play: Bool) {
        let sentence = snapshot.sentence
        let revision = snapshot.revision
        open(text: text, source: sourceName)
        pendingResume = Resume(sentence: sentence, play: play, after: revision)
        resumeIfReady()
    }

    /// Où reprendre après une modification, appliqué dès que le nouveau
    /// document a ses phrases.
    struct Resume: Equatable {
        let sentence: UInt32
        let play: Bool
        /// La révision de l'ancien document : on attend la suivante.
        let after: UInt64
    }
    @ObservationIgnored private(set) var pendingResume: Resume?

    private func resumeIfReady() {
        guard let resume = pendingResume, snapshot.revision > resume.after, document.sentenceCount > 0,
              resume.sentence < document.sentenceCount || document.complete else { return }
        pendingResume = nil
        let target = min(resume.sentence, document.sentenceCount - 1)
        if target > 0 { send(.seekTo(sentence: target)) }
        if resume.play != snapshot.playing { send(resume.play ? .togglePlay : .pause) }
    }

    /// Une image copiée, déjà vérifiée : le moteur la passe à l'OCR
    /// (plusieurs secondes), hors du fil principal.
    func openImage(_ data: Data) async throws {
        message = nil
        sourceText = nil
        sourceName = "image"
        pendingResume = nil
        let reader = reader
        try await Task.detached { try reader.openImage(bytes: data) }.value
        wake()
    }

    /// Un fichier donné à sillage (`sillage fichier.pdf`, « Ouvrir avec ») : le
    /// moteur l'extrait (PDF, texte, image) et reprend où on s'était arrêté.
    /// L'extraction peut durer : hors du fil principal.
    func openFile(_ url: URL) async throws {
        message = nil
        sourceText = nil
        sourceName = url.lastPathComponent
        pendingResume = nil
        let reader = reader
        let path = url.path
        try await Task.detached { try reader.openFile(path: path) }.value
        wake()
    }

    func prepareVoice() {
        reader.prepareVoice()
        wake()
    }

    // MARK: - Réglages

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

    // MARK: - Sortie audio

    /// La sortie choisie (vide : celle du système), et les sorties branchées.
    /// Le menu, les réglages et le panneau la lisent et la changent ici.
    var outputDevice: String { audioOutputs.device }
    var outputs: AudioOutputsView { audioOutputs.available }

    /// Relit les sorties branchées, hors du fil principal : à chaque sortie
    /// branchée ou retirée (`AudioDeviceWatcher`), jamais à chaque image.
    func refreshOutputs() {
        audioOutputs.refresh()
    }

    /// La sortie retenue dans `config.toml`, relue au lancement.
    func loadOutput() async {
        audioOutputs.select(await loadSettings().outputDevice)
        await audioOutputs.reload()
    }

    /// Choisit la sortie : le son y passe tout de suite et n'en sort plus.
    func chooseOutput(_ name: String) {
        guard name != outputDevice else { return }
        let previous = outputDevice
        audioOutputs.select(name)
        var patch = SettingsPatch.empty
        patch.outputDevice = name
        Task {
            do {
                _ = try await updateSettings(patch)
            } catch {
                audioOutputs.select(previous)
                message = String(localized: "Audio output not saved: \(error.localizedDescription)")
            }
        }
    }

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

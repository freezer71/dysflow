import Foundation
import MediaPlayer

/// Ce que « À l'écoute » montre. Durée et position au rythme 1× (temps
/// estimé × vitesse), la vitesse passe par le débit : macOS fait avancer la
/// position seul, sans qu'on la republie chaque seconde.
///
/// Sens inverse : `Transport.sentence(at:snapshot:document:)` divise la
/// position glissée sur la timeline par la vitesse. Les deux conventions
/// vont ensemble (`testThePublishedPositionScrubsBackToTheSameSentence`).
struct NowPlayingInfo: Equatable {
    var title: String
    var duration: Double?
    var elapsed: Double?
    var speed: Double
    var playing: Bool

    /// Rien sans document, ni quand la voix est au repos.
    static func from(_ s: Snapshot, document: DocView) -> NowPlayingInfo? {
        guard s.hasDocument else { return nil }
        if case .resting = s.voice, !s.playing { return nil }
        let speed = Double(s.speed)
        // × vitesse ; `Transport.sentence` fait le ÷ vitesse inverse.
        func content(_ t: Double?) -> Double? { t.flatMap { $0.isFinite ? $0 * speed : nil } }
        return NowPlayingInfo(title: document.title, duration: content(s.total),
                              elapsed: content(s.elapsed), speed: speed, playing: s.playing)
    }

    var dictionary: [String: Any] {
        var d: [String: Any] = [
            MPMediaItemPropertyTitle: title,
            // `as Double` : typé contre `Any`, le 0 serait un `Int`.
            MPNowPlayingInfoPropertyPlaybackRate: (playing ? speed : 0) as Double,
            MPNowPlayingInfoPropertyDefaultPlaybackRate: speed,
            MPNowPlayingInfoPropertyMediaType: MPNowPlayingInfoMediaType.audio.rawValue,
        ]
        if let duration { d[MPMediaItemPropertyPlaybackDuration] = duration }
        if let elapsed { d[MPNowPlayingInfoPropertyElapsedPlaybackTime] = elapsed }
        return d
    }
}

/// Publie l'état de la lecture à « À l'écoute » et y branche les commandes
/// média. Appelé à chaque instantané publié (`ReaderModel.onChange`), il
/// n'écrit qu'aux changements d'état : lecture, pause, phrase, vitesse,
/// durée, document, saut de position — jamais pour le seul temps qui passe
/// (`onChange` part pourtant jusqu'à ~17 fois par seconde, panneau affiché).
@MainActor
final class NowPlaying {
    typealias Schedule = @MainActor (TimeInterval, @escaping @MainActor () -> Void) -> @MainActor () -> Void

    private let center: NowPlayingCenter
    private let remote: RemoteCommandSource
    private let transport: Transport
    private let restDelay: @MainActor () async -> TimeInterval?
    private let schedule: Schedule
    private let refresh: @MainActor () -> Void
    /// Ce qui a été publié, sans la position pendant la lecture.
    private var last: Key?
    /// Le temps estimé du dernier instantané reçu : un écart hors de
    /// l'avance normale est un saut à republier.
    private var lastElapsed: Double?
    private var installed = false
    private var cancelRestCheck: (@MainActor () -> Void)?
    private var restGeneration = 0
    /// Une par changement d'état : les tests le comptent.
    private(set) var publications = 0

    private struct Key: Equatable {
        var title: String
        var duration: Double?
        var speed: Double
        var playing: Bool
        var sentence: UInt32
        var revision: UInt64
        /// En pause rien n'avance seul : la position fait partie de l'état.
        var pausedAt: Double?
    }

    /// Pendant la lecture, le temps estimé avance d'environ 1 s par seconde
    /// (publié arrondi à la seconde, relu de 17 fois par seconde à une fois
    /// par seconde). Un recul de plus de 1,5 s ou une avance de plus de 5 s
    /// entre deux instantanés est un saut (phrase reprise, mot choisi).
    private static func jumped(from before: Double?, to now: Double?) -> Bool {
        guard let before, let now else { return false }
        let step = now - before
        return step < -1.5 || step > 5
    }

    init(center: NowPlayingCenter, remote: RemoteCommandSource, transport: Transport,
         restDelay: @escaping @MainActor () async -> TimeInterval?,
         schedule: @escaping Schedule = NowPlaying.onMainQueue,
         refresh: @escaping @MainActor () -> Void) {
        self.center = center
        self.remote = remote
        self.transport = transport
        self.restDelay = restDelay
        self.schedule = schedule
        self.refresh = refresh
    }

    func update(_ s: Snapshot, document: DocView) {
        guard let info = NowPlayingInfo.from(s, document: document) else {
            clear()
            return
        }
        let key = Key(title: info.title, duration: info.duration, speed: info.speed,
                      playing: info.playing, sentence: s.sentence, revision: s.revision,
                      pausedAt: info.playing ? nil : s.elapsed)
        let jump = info.playing && Self.jumped(from: lastElapsed, to: s.elapsed)
        lastElapsed = s.elapsed
        guard key != last || jump else { return }
        last = key
        if !installed {
            installed = true
            let transport = transport
            remote.install { action in transport.perform(action) }
        }
        remote.setEnabled(true)
        center.nowPlayingInfo = info.dictionary
        center.playbackState = info.playing ? .playing : .paused
        publications += 1
        if info.playing { stopRestCheck() } else { armRestCheck() }
    }

    private func clear() {
        stopRestCheck()
        lastElapsed = nil
        guard last != nil else { return }
        last = nil
        center.nowPlayingInfo = nil
        center.playbackState = .stopped
        remote.setEnabled(false)
    }

    /// En pause, une seule relecture différée, au moment où le moteur met la
    /// voix au repos : elle verra `.resting` et retirera l'affichage. Pas de
    /// minuterie qui revient.
    private func armRestCheck() {
        stopRestCheck()
        let generation = restGeneration
        Task { @MainActor [weak self] in
            guard let self, let delay = await self.restDelay(),
                  generation == self.restGeneration else { return }
            let refresh = self.refresh
            self.cancelRestCheck = self.schedule(delay) { refresh() }
        }
    }

    private func stopRestCheck() {
        restGeneration &+= 1
        cancelRestCheck?()
        cancelRestCheck = nil
    }

    /// Le moment de relire l'état après une pause : la durée « Garder la
    /// voix chargée », plus 2 s de marge. Nil : la voix reste chargée.
    nonisolated static func restDelay(keepVoiceMinutes: Int64) -> TimeInterval? {
        keepVoiceMinutes < 0 ? nil : TimeInterval(keepVoiceMinutes) * 60 + 2
    }

    static func onMainQueue(_ delay: TimeInterval,
                            _ work: @escaping @MainActor () -> Void) -> @MainActor () -> Void {
        let item = DispatchWorkItem { MainActor.assumeIsolated { work() } }
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: item)
        return { item.cancel() }
    }
}

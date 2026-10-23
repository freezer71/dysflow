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

    /// Rien sans document, ni quand la voix est au repos, ni en pause
    /// lecteur fermé (fermer met en pause : la lecture est finie).
    static func from(_ s: Snapshot, document: DocView, readerShown: Bool = true) -> NowPlayingInfo? {
        guard s.hasDocument else { return nil }
        if case .resting = s.voice, !s.playing { return nil }
        if !readerShown, !s.playing { return nil }
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
    /// Le dernier instantané reçu : relu quand le lecteur s'ouvre ou se ferme.
    private var received: (snapshot: Snapshot, document: DocView)?

    /// Le lecteur est-il affiché ? Posé par le panneau. Fermé en pause, rien
    /// n'est montré : les touches média reviennent aux autres applications.
    var readerShown = true {
        didSet {
            guard readerShown != oldValue, let received else { return }
            update(received.snapshot, document: received.document)
        }
    }

    private struct Key: Equatable {
        var title: String
        /// Voulue dans la clé : le moteur affine son estimation de la durée
        /// au fil de la lecture (environ une fois par phrase), et chaque
        /// affinage republie pour garder la timeline juste.
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
    /// Faux négatifs acceptés : une phrase reprise moins de 1,5 s après son
    /// début, ou un saut en avant de moins de 5 s dans la même phrase, ne
    /// republient pas ; l'écart reste petit et la phrase suivante recale.
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
        received = (s, document)
        guard let info = NowPlayingInfo.from(s, document: document, readerShown: readerShown) else {
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

    /// En pause, une seule relecture différée à la fois, au moment où le
    /// moteur met la voix au repos : elle verra `.resting` et retirera
    /// l'affichage. Si la voix est encore chargée (une commande en pause a
    /// relancé le compte du moteur sans rien changer de publié, ou « Garder
    /// la voix chargée » a été allongé), elle en prévoit une autre, avec le
    /// délai relu : fenêtre masquée en pause, rien d'autre ne relirait
    /// l'état. Cela converge : sans activité, le moteur finit par reposer
    /// la voix. Rien n'est prévu pendant la lecture ni sans affichage.
    private func armRestCheck() {
        stopRestCheck()
        let generation = restGeneration
        Task { @MainActor [weak self] in
            guard let self, let delay = await self.restDelay(),
                  generation == self.restGeneration else { return }
            self.cancelRestCheck = self.schedule(delay) { [weak self] in
                self?.restCheckFired(generation: generation)
            }
        }
    }

    private func restCheckFired(generation: Int) {
        // Elle a eu lieu : plus rien à annuler.
        cancelRestCheck = nil
        refresh()
        // Un changement publié pendant la relecture a déjà décidé (repos :
        // retiré ; nouvelle pause : relecture prévue ; lecture : rien).
        guard generation == restGeneration, let last, !last.playing else { return }
        armRestCheck()
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

import Foundation

/// Le rythme auquel le modèle relit l'état du lecteur : à chaque image, au
/// coup d'œil, à la seconde ou jamais, selon ce qui est montré et ce qui
/// avance. Une source d'images (l'écran), une minuterie de relecture et la
/// fin de la grâce d'une commande ; chacune appelle `refresh`.
@MainActor
final class RefreshSchedule {
    /// Ce que la fenêtre montre de l'état : rien (arrière-plan), la barre
    /// du mode petit, ou le texte (moyen, grand).
    enum Presence: Equatable { case hidden, bar, full }

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

    /// Après une commande, l'effet n'arrive que quelques ms plus tard : le suivi
    /// ne peut pas s'arrêter avant cette échéance.
    static let gracePeriod: TimeInterval = 0.5
    static let glanceInterval: TimeInterval = 0.25
    static let secondInterval: TimeInterval = 1

    /// La décision de suivi, sans état : testable. Quelque chose avance
    /// (lecture, commande récente, voix qui se charge, téléchargement) : à
    /// chaque image si le texte est affiché, au coup d'œil pour la barre,
    /// à la seconde en arrière-plan. En pause, seul le texte affiché garde un
    /// coup d'œil — le panneau flotte, il peut rester visible des heures.
    static func cadence(presence: Presence, playing: Bool, grace: Bool, loading: Bool,
                        download: DownloadView?) -> Cadence {
        if playing || grace || loading || ReaderModel.isDownloading(download) {
            switch presence {
            case .full: return .screen
            case .bar: return .glance
            case .hidden: return .second
            }
        }
        return presence == .full ? .glance : .none
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

    private let frames: FrameSource
    private let now: () -> Date
    /// La relecture de l'état : à chaque image, à chaque coup de minuterie,
    /// à la fin de la grâce.
    var refresh: @MainActor () -> Void = {}
    private var keepAliveUntil = Date.distantPast
    private(set) var cadence: Cadence = .none
    private var pollTimer: Timer?
    /// La fin de la grâce d'une commande (voir `scheduleGraceEnd`).
    private var graceTimer: Timer?

    init(frames: FrameSource, now: @escaping () -> Date) {
        self.frames = frames
        self.now = now
        frames.onFrame = { [weak self] in self?.refresh() }
    }

    /// La boucle principale retient les minuteries : elles partent avec le
    /// modèle (seul à garder ce rythme).
    deinit {
        pollTimer?.invalidate()
        graceTimer?.invalidate()
    }

    /// Le suivi à la cadence de l'écran tourne-t-il ?
    var isTicking: Bool { frames.isRunning }

    /// Le suivi à la cadence de l'écran.
    func start() { frames.start() }
    func stop() { frames.stop() }

    /// Une commande vient de partir : le suivi tient jusqu'à la fin de la
    /// grâce, même sans rien qui avance.
    func wake() {
        keepAliveUntil = now().addingTimeInterval(Self.gracePeriod)
        scheduleGraceEnd()
    }

    /// Règle images et minuterie sur ce qui est montré et ce qui avance.
    func update(presence: Presence, snapshot: Snapshot) {
        let loading: Bool
        if case .loading = snapshot.voice { loading = true } else { loading = false }
        let next = Self.cadence(presence: presence, playing: snapshot.playing,
                                grace: now() < keepAliveUntil, loading: loading,
                                download: snapshot.download)
        cadence = next
        if next == .screen { start() } else { stop() }
        if let interval = Self.pollInterval(for: next) { startPolling(every: interval) } else { stopPolling() }
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
}

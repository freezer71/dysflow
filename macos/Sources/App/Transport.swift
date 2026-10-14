import Foundation

/// Ce que l'on demande à la lecture hors du panneau : touches média, AirPods,
/// Centre de contrôle, menu de la barre des menus, raccourcis globaux.
enum RemoteAction: Equatable {
    case play, pause, toggle, next, previous
    /// Un point de la timeline d'« À l'écoute », en secondes au rythme 1×.
    case seek(TimeInterval)
}

/// Traduit ces demandes en commandes du lecteur. Sans document, rien ne part
/// et la demande est dite non servie.
@MainActor
struct Transport {
    let snapshot: () -> Snapshot
    let document: () -> DocView
    let send: (ReaderCommand) -> Void

    init(snapshot: @escaping () -> Snapshot, document: @escaping () -> DocView,
         send: @escaping (ReaderCommand) -> Void) {
        self.snapshot = snapshot
        self.document = document
        self.send = send
    }

    /// Fenêtre masquée et lecture en pause, le modèle ne suit rien : l'état
    /// est relu à la demande.
    init(model: ReaderModel) {
        self.init(snapshot: { model.refresh(); return model.snapshot },
                  document: { model.document },
                  send: { model.send($0) })
    }

    /// Vrai si la demande a servi.
    @discardableResult
    func perform(_ action: RemoteAction) -> Bool {
        let s = snapshot()
        guard s.hasDocument else { return false }
        switch action {
        case .play:
            if !s.playing { send(.togglePlay) }
        case .pause:
            if s.playing { send(.pause) }
        case .toggle:
            send(.togglePlay)
        case .next:
            send(.sentences(delta: 1))
        case .previous:
            send(.sentences(delta: -1))
        case .seek(let position):
            guard let target = Self.sentence(at: position, snapshot: s, document: document()) else { return false }
            send(.seekTo(sentence: target))
        }
        return true
    }

    /// La phrase dont le début est le plus proche de `position`, en secondes
    /// au rythme 1× (durée estimée × vitesse, comme « À l'écoute » les
    /// compte). Le temps est proportionnel aux mots, comme l'estimation du
    /// moteur. Avant le début : la première ; à la fin ou au-delà : la
    /// dernière. Nil si la durée n'est pas encore connue ou le document vide.
    static func sentence(at position: TimeInterval, snapshot s: Snapshot, document: DocView) -> UInt32? {
        let sentences = document.blocks.flatMap(\.sentences)
        guard let first = sentences.first, let last = sentences.last,
              let total = s.total, total.isFinite, total > 0, position.isFinite else { return nil }
        let fraction = position / (total * Double(max(s.speed, 0.1)))
        if fraction >= 1 { return last.id }
        if fraction <= 0 { return first.id }
        let words = sentences.map(\.words.count)
        let target = fraction * Double(words.reduce(0, +))
        var start = 0
        var best = first.id
        var bestDistance = Double.infinity
        for (sentence, count) in zip(sentences, words) {
            let distance = abs(Double(start) - target)
            if distance < bestDistance {
                best = sentence.id
                bestDistance = distance
            }
            start += count
        }
        return best
    }
}

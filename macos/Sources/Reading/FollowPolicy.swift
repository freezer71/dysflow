import Foundation

/// La vue suit-elle la lecture ?
///
/// Relire en faisant défiler le texte est l'usage principal : dès que l'on
/// fait défiler à la main, la vue cesse de ramener la ligne lue. Elle reprend
/// à la prochaine commande de lecture, ou quand la voix passe à une nouvelle
/// phrase après quelques secondes sans défilement.
struct FollowPolicy {
    static let quietDelay: TimeInterval = 4

    private(set) var suspended = false
    private var lastUserScroll = Date.distantPast

    mutating func userScrolled(at now: Date) {
        suspended = true
        lastUserScroll = now
    }

    mutating func resume() {
        suspended = false
    }

    mutating func shouldFollow(newSentence: Bool, at now: Date) -> Bool {
        if suspended, newSentence, now.timeIntervalSince(lastUserScroll) >= Self.quietDelay {
            suspended = false
        }
        return !suspended
    }
}

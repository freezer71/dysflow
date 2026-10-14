import Foundation
import MediaPlayer

/// Le centre « À l'écoute », derrière un protocole : les tests passent un faux.
protocol NowPlayingCenter: AnyObject {
    var nowPlayingInfo: [String: Any]? { get set }
    var playbackState: MPNowPlayingPlaybackState { get set }
}

extension MPNowPlayingInfoCenter: NowPlayingCenter {}

/// Les commandes média (clavier, AirPods, Centre de contrôle).
protocol RemoteCommandSource: AnyObject {
    /// Branche les commandes ; `handler` rend vrai si la commande a servi.
    func install(_ handler: @escaping @MainActor @Sendable (RemoteAction) -> Bool)
    func setEnabled(_ enabled: Bool)
}

/// Les commandes du système, branchées sur `MPRemoteCommandCenter`.
final class SystemRemoteCommands: RemoteCommandSource {
    private let center = MPRemoteCommandCenter.shared()

    private var commands: [MPRemoteCommand] {
        [center.playCommand, center.pauseCommand, center.togglePlayPauseCommand,
         center.nextTrackCommand, center.previousTrackCommand, center.changePlaybackPositionCommand]
    }

    func install(_ handler: @escaping @MainActor @Sendable (RemoteAction) -> Bool) {
        let pairs: [(MPRemoteCommand, RemoteAction)] = [
            (center.playCommand, .play), (center.pauseCommand, .pause),
            (center.togglePlayPauseCommand, .toggle),
            (center.nextTrackCommand, .next), (center.previousTrackCommand, .previous),
        ]
        for (command, action) in pairs {
            command.addTarget { _ in
                Self.onMain { handler(action) } ? .success : .noActionableNowPlayingItem
            }
        }
        center.changePlaybackPositionCommand.addTarget { event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
            let position = event.positionTime
            return Self.onMain { handler(.seek(position)) } ? .success : .commandFailed
        }
    }

    func setEnabled(_ enabled: Bool) {
        for command in commands { command.isEnabled = enabled }
    }

    /// macOS appelle les commandes sur le fil principal ; sinon on l'y attend.
    private static func onMain(_ body: @MainActor @Sendable () -> Bool) -> Bool {
        if Thread.isMainThread { return MainActor.assumeIsolated { body() } }
        return DispatchQueue.main.sync { MainActor.assumeIsolated { body() } }
    }
}

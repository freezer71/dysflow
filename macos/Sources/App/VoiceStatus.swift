import SwiftUI

/// Ce qu'on dit de la voix, seulement quand elle n'est pas prête.
enum VoiceStatus: Equatable {
    case downloading(received: UInt64, total: UInt64)
    case failed(String)
    case unavailable(String)
    case fallback(String)
    case preparing

    /// Un échec d'abord (il attend « Réessayer »), puis le téléchargement
    /// (la voix du système lit pendant ce temps : pas de repli à dire en
    /// plus), puis la voix muette, le repli, la première préparation.
    static func current(snapshot s: Snapshot, preparing: Bool, setupError: String?) -> VoiceStatus? {
        if let failed = s.download?.failed { return .failed(failed) }
        if let setupError { return .failed(setupError) }
        if let d = s.download { return .downloading(received: d.received, total: d.total) }
        if case .unavailable(let reason) = s.voice { return .unavailable(reason) }
        if case .fallback(let reason) = s.voice { return .fallback(reason) }
        if preparing { return .preparing }
        return nil
    }

    var text: String {
        switch self {
        case .downloading(let received, let total):
            String(localized: "Installing the voice: \(SizeFormat.bytes(received)) of \(SizeFormat.bytes(total)) — the system voice reads in the meantime")
        case .failed(let why): String(localized: "The voice could not be installed: \(why)")
        case .unavailable(let why): String(localized: "Voice unavailable: \(why)")
        case .fallback(let why): String(localized: "System voice as a fallback: \(why)")
        case .preparing: String(localized: "Preparing the voice (first time)…")
        }
    }

    var canRetry: Bool {
        if case .failed = self { return true }
        return false
    }
}

/// La ligne, dans le lecteur et en bas des réglages : rien quand la voix va bien.
struct VoiceStatusLine: View {
    let model: ReaderModel
    var color: Color = Palette.muted

    var body: some View {
        if let status = VoiceStatus.current(snapshot: model.snapshot, preparing: model.voicePreparation.isRunning,
                                            setupError: model.voiceSetupError) {
            HStack(spacing: 8) {
                if case .downloading(let received, let total) = status {
                    ProgressView(value: Double(received), total: Double(max(total, 1)))
                        .frame(width: 100)
                        .accessibilityLabel("Voice installation")
                        .accessibilityValue(SettingFormat.percent(Float(received) / Float(max(total, 1))))
                }
                Text(status.text)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("etat-voix")
                if status.canRetry {
                    Button("Retry") { Task { await model.installDefaultVoice() } }
                        .buttonStyle(.notion)
                        .accessibilityLabel("Retry installing the voice")
                        .accessibilityIdentifier("reessayer-voix")
                }
            }
            .font(.system(size: 12))
            .foregroundStyle(color)
            .accessibilityElement(children: .contain)
        }
    }
}

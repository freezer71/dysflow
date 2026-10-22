import SwiftUI

/// La justesse et la vitesse publiées d'un modèle de dictée : jamais
/// estimées, absentes quand rien n'est publié (la dictée de macOS).
///
/// Sauf mention, le classement Open ASR du 21-10-2026 (même banc, même
/// normalisation) : https://huggingface.co/spaces/hf-audio/open_asr_leaderboard
/// — taux de mots faux sur FLEURS fr (`hf-audio/multilingual_evals`), moyenne
/// anglaise et RTFx sur GPU H200 (`hf-audio/open-asr-leaderboard-results`).
/// Chiffres des poids d'origine : sillage en utilise des versions réduites
/// (8 bits, 4 bits).
struct ModelScore: Equatable {
    /// Taux de mots faux, en %.
    var wer: Double
    /// La langue mesurée : « fr », ou « en » pour un modèle anglais.
    var language: String
    var source: Source
    /// Secondes d'audio transcrites par seconde de calcul (H200) ; `nil` :
    /// non publiée.
    var rtfx: Double?

    enum Source { case leaderboard, whisperPaper }

    /// Le plus rapide du classement (Parakeet v3) : la vitesse en est la part.
    static let fastest: Double = 6132

    static func of(_ id: String) -> ModelScore? {
        switch id {
        case "parakeet_tdt_v3": ModelScore(wer: 4.68, language: "fr", source: .leaderboard, rtfx: 6132)
        case "phonon_2": ModelScore(wer: 6.87, language: "en", source: .leaderboard, rtfx: 1967)
        // Whisper tiny et small ne sont pas au classement : FLEURS fr du
        // papier de Whisper (2022, table 13), autre normalisation.
        case "whisper_tiny_4bit": ModelScore(wer: 41.4, language: "fr", source: .whisperPaper, rtfx: nil)
        case "whisper_small_4bit": ModelScore(wer: 15.0, language: "fr", source: .whisperPaper, rtfx: nil)
        case "whisper_large_v3_turbo_4bit": ModelScore(wer: 4.90, language: "fr", source: .leaderboard, rtfx: 794)
        case "canary_1b_v2_q8": ModelScore(wer: 4.35, language: "fr", source: .leaderboard, rtfx: 1821)
        case "voxtral_realtime_4bit": ModelScore(wer: 8.19, language: "fr", source: .leaderboard, rtfx: 103)
        case "parakeet_tdt_v2": ModelScore(wer: 6.02, language: "en", source: .leaderboard, rtfx: 5988)
        case "nemotron_35_asr_streaming_8bit": ModelScore(wer: 9.86, language: "fr", source: .leaderboard, rtfx: 1343)
        case "qwen3_asr_06b_8bit": ModelScore(wer: 7.06, language: "fr", source: .leaderboard, rtfx: 745)
        case "qwen3_asr_17b_4bit": ModelScore(wer: 4.06, language: "fr", source: .leaderboard, rtfx: 819)
        case "cohere_transcribe_8bit": ModelScore(wer: 4.33, language: "fr", source: .leaderboard, rtfx: 902)
        default: nil
        }
    }

    /// 100 moins le taux de mots faux.
    var accuracy: Int { Int((100 - wer).rounded()) }

    /// La part de la vitesse du plus rapide, au moins 1 %.
    var speed: Int? { rtfx.map { max(1, Int((100 * $0 / Self.fastest).rounded())) } }

    var accuracyText: String {
        language == "fr" ? String(localized: "Accuracy \(accuracy)%")
            : String(localized: "Accuracy (English) \(accuracy)%")
    }

    var speedText: String? { speed.map { String(localized: "Speed \($0)%") } }

    /// Ce que VoiceOver dit des deux jauges.
    var spoken: String {
        [speedText, accuracyText].compactMap { $0 }.joined(separator: ", ")
    }

    var accuracyHelp: String {
        let wer = wer.formatted(.number.precision(.fractionLength(0...2)).locale(UILanguage.locale))
        switch (source, language) {
        case (.whisperPaper, _):
            return String(localized: "\(wer)% of words wrong on FLEURS French (Whisper paper, 2022: not on the Open ASR leaderboard)")
        case (.leaderboard, "fr"):
            return String(localized: "\(wer)% of words wrong on FLEURS French (Open ASR leaderboard, 21 Oct 2026)")
        default:
            return String(localized: "\(wer)% of words wrong, English average (Open ASR leaderboard, 21 Oct 2026). No French figure is published")
        }
    }

    var speedHelp: String? {
        guard let rtfx, let speed else { return nil }
        let times = Int(rtfx.rounded())
        return String(localized: "\(times)× real time on a server GPU (H200, Open ASR leaderboard, 21 Oct 2026): \(speed)% of the fastest. Not measured on a Mac")
    }
}

/// « ⚡ ▬▬▬ 100 %   ◎ ▬▬▬ 95 % » sous le nom d'un modèle de dictée : une
/// icône, une barre, le pourcentage ; les mots et la source au survol (et
/// pour VoiceOver). Largeurs fixes : les barres s'alignent d'une ligne à
/// l'autre, et une vitesse absente garde sa place. Rien pour un modèle sans
/// chiffre publié.
struct ModelScoreLine: View {
    let id: String

    var body: some View {
        if let score = ModelScore.of(id) {
            HStack(spacing: 16) {
                ScoreGauge(symbol: "bolt.fill", value: score.speed, tag: nil)
                    .help(score.speedHelp ?? "")
                ScoreGauge(symbol: "scope", value: score.accuracy, tag: score.language == "fr" ? nil : "EN")
                    .help(score.accuracyHelp)
            }
            .padding(.top, 2)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(score.spoken)
        }
    }

}

/// Une jauge : l'icône, une barre de 0 à 100, le pourcentage ; `nil` : la
/// place, vide.
private struct ScoreGauge: View {
    let symbol: String
    let value: Int?
    /// La langue mesurée quand ce n'est pas le français (« EN »).
    let tag: String?

    static let barWidth: CGFloat = 48

    var body: some View {
        HStack(spacing: 6) {
            if let value {
                Image(systemName: symbol)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(Palette.accent)
                    .frame(width: 12)
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.muted.opacity(0.25))
                    Capsule().fill(Palette.accent)
                        .frame(width: Self.barWidth * CGFloat(min(100, max(0, value))) / 100)
                }
                .frame(width: Self.barWidth, height: 4)
                Text(String(localized: "\(value)%"))
                    .font(Typo.detail.monospacedDigit())
                    .foregroundStyle(Palette.muted)
                    .lineLimit(1)
                    .frame(width: 40, alignment: .leading)
                if let tag {
                    Text(tag)
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(Palette.muted)
                        .padding(.horizontal, 3)
                        .overlay(RoundedRectangle(cornerRadius: 3).stroke(Palette.muted.opacity(0.5)))
                }
            }
        }
        .frame(width: 12 + Self.barWidth + 40 + 2 * 6 + 22, alignment: .leading)
        .fixedSize()
    }
}

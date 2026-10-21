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

/// « ⚡ Vitesse 100 % · ◎ Justesse 95 % » sous le nom d'un modèle de
/// dictée ; la source au survol. Rien pour un modèle sans chiffre publié.
struct ModelScoreLine: View {
    let id: String

    var body: some View {
        if let score = ModelScore.of(id) {
            HStack(spacing: 10) {
                if let speed = score.speedText {
                    Label(speed, systemImage: "bolt.fill")
                        .help(score.speedHelp ?? "")
                }
                Label(score.accuracyText, systemImage: "scope")
                    .help(score.accuracyHelp)
            }
            .labelStyle(ScoreLabelStyle())
            .font(Typo.detail.monospacedDigit())
            .foregroundStyle(Palette.muted)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Self.spoken(score))
        }
    }

    static func spoken(_ score: ModelScore) -> String {
        [score.speedText, score.accuracyText].compactMap { $0 }.joined(separator: ", ")
    }
}

private struct ScoreLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 3) {
            configuration.icon.foregroundStyle(Palette.accent)
            configuration.title
        }
    }
}

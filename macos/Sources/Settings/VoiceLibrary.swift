import Foundation
import Observation

/// Une voix de la bibliothèque, telle que la feuille la montre : ce que le
/// moteur en dit, plus le genre et la qualité qu'AVFoundation donne aux
/// voix de macOS.
struct VoiceLibraryEntry: Identifiable, Equatable {
    let modelID: String
    let modelLabel: String
    let voiceID: String
    let label: String
    let language: String
    let accent: String
    /// « femme », « homme », vide si on ne le sait pas.
    let gender: String
    /// Voix de macOS : 3 Premium, 2 Améliorée, 1 sinon ; Kokoro : 0.
    let quality: Int
    let recommended: Bool
    let modelActive: Bool
    let active: Bool

    var id: String { "\(modelID)/\(voiceID)" }

    /// L'étiquette de qualité d'une voix de macOS.
    var qualityBadge: String? {
        switch quality {
        case 3: "Premium"
        case 2: "Améliorée"
        default: nil
        }
    }

    /// La ligne sous le nom : modèle, accent, genre.
    var details: String {
        [modelLabel, accent, gender].filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// La bibliothèque des voix, sans interface : ses lignes, leurs filtres et
/// leur ordre.
enum VoiceLibrary {
    /// Les filtres de la feuille ; vide : tout.
    struct Filter: Equatable {
        var model = ""
        var accent = ""
        var gender = ""
        var search = ""
    }

    /// Les lignes : les voix de macOS reçoivent genre et qualité
    /// d'AVFoundation, rapprochés par leur nom (`VoiceSection.quality`).
    static func entries(_ voices: [LibraryVoice], macVoices: [VoiceSection.MacVoice]) -> [VoiceLibraryEntry] {
        let macIDs = voices.filter { $0.modelId == "system" }.map(\.voiceId)
        let quality = VoiceSection.quality(of: macIDs, voices: macVoices)
        let genders = VoiceSection.genders(of: macIDs, voices: macVoices)
        return voices.map { v in
            let system = v.modelId == "system"
            return VoiceLibraryEntry(
                modelID: v.modelId, modelLabel: v.modelLabel, voiceID: v.voiceId, label: v.label,
                language: v.language, accent: v.accent,
                gender: system ? (genders[v.voiceId] ?? "") : v.gender,
                quality: system ? (quality[v.voiceId] ?? 1) : 0,
                recommended: v.recommended, modelActive: v.modelActive, active: v.active)
        }
    }

    /// Le modèle qui lit d'abord, les voix recommandées ensuite, puis par nom.
    static func ordered(_ entries: [VoiceLibraryEntry]) -> [VoiceLibraryEntry] {
        entries.sorted { a, b in
            if a.modelActive != b.modelActive { return a.modelActive }
            if a.modelLabel != b.modelLabel {
                return a.modelLabel.localizedStandardCompare(b.modelLabel) == .orderedAscending
            }
            if a.recommended != b.recommended { return a.recommended }
            return a.label.localizedStandardCompare(b.label) == .orderedAscending
        }
    }

    /// Les lignes qui passent les filtres, dans l'ordre de la bibliothèque.
    /// La recherche porte sur le nom, sans casse ni accents.
    static func filtered(_ entries: [VoiceLibraryEntry], _ filter: Filter) -> [VoiceLibraryEntry] {
        let search = filter.search.trimmingCharacters(in: .whitespacesAndNewlines)
        return ordered(entries).filter { e in
            (filter.model.isEmpty || e.modelID == filter.model)
                && (filter.accent.isEmpty || e.accent == filter.accent)
                && (filter.gender.isEmpty || e.gender == filter.gender)
                && (search.isEmpty
                    || e.label.range(of: search, options: [.caseInsensitive, .diacriticInsensitive]) != nil)
        }
    }

    static let all = "Tous"

    /// « Tous », puis chaque modèle présent, dans l'ordre de la bibliothèque.
    static func modelOptions(_ entries: [VoiceLibraryEntry]) -> [(title: String, value: String)] {
        var seen = Set<String>()
        let models = ordered(entries).filter { seen.insert($0.modelID).inserted }
        return [(title: all, value: "")] + models.map { (title: $0.modelLabel, value: $0.modelID) }
    }

    /// « Tous », puis les accents présents, par ordre alphabétique.
    static func accentOptions(_ entries: [VoiceLibraryEntry]) -> [(title: String, value: String)] {
        let accents = Set(entries.map(\.accent).filter { !$0.isEmpty })
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
        return [(title: all, value: "")] + accents.map { (title: $0, value: $0) }
    }

    static let genderOptions: [(title: String, value: String)] =
        [(title: all, value: ""), (title: "Femme", value: "femme"), (title: "Homme", value: "homme")]

    /// Ce que VoiceOver dit d'une ligne : tout, en clair.
    static func accessibilityLabel(_ e: VoiceLibraryEntry) -> String {
        var parts = [e.label, e.modelLabel, e.accent, e.gender].filter { !$0.isEmpty }
        if let badge = e.qualityBadge { parts.append(badge) }
        if e.recommended { parts.append("recommandée") }
        if e.active { parts.append("voix utilisée") }
        return parts.joined(separator: ", ")
    }
}

/// Ce que la bibliothèque demande au moteur. `ReaderModel` le fournit ; les
/// tests le simulent.
@MainActor
protocol VoiceLibraryBackend: AnyObject {
    /// Les voix ; la voix par défaut de macOS prend l'accent de la locale du Mac.
    func loadVoiceLibrary() async -> [LibraryVoice]
    func previewLibraryVoice(model: String, voice: String, text: String, speed: Float) async throws
    func stopPreview() async
    func releaseVoicePreview() async
}

extension ReaderModel: VoiceLibraryBackend {}

/// L'état de la feuille « Bibliothèque des voix » : les lignes, les filtres,
/// la ligne choisie, le texte et la vitesse de l'aperçu (gardés tant que la
/// feuille est ouverte), la ligne qui sonne.
@MainActor
@Observable
final class VoiceLibraryStore {
    @ObservationIgnored private let backend: VoiceLibraryBackend
    @ObservationIgnored private let macVoices: () -> [VoiceSection.MacVoice]
    @ObservationIgnored private let errorNote: ErrorNote
    /// L'aperçu demandé a été vu en route : le prochain repos dit sa fin.
    @ObservationIgnored private var seenActive = false

    private(set) var entries: [VoiceLibraryEntry] = []
    private(set) var loaded = false
    var filter = VoiceLibrary.Filter()
    /// La ligne choisie (`VoiceLibraryEntry.id`).
    var selection: String?
    /// Le texte à lire, prérempli avec la phrase d'exemple.
    var text: String
    /// La vitesse de l'aperçu seulement.
    var speed: Float
    /// La ligne dont l'aperçu est demandé ou sonne.
    private(set) var playing: String?
    /// Un aperçu a été demandé depuis la feuille : l'échec du moteur est le sien.
    private(set) var requested = false
    var error: String? { errorNote.text }

    static let speedRange: ClosedRange<Float> = 0.5...3
    /// Au-delà, le moteur coupe le texte de l'essai (`MAX_PREVIEW_CHARS`).
    static let maxCharacters = 500

    /// Ce que le compteur sous « Texte à lire » dit.
    static func countLine(_ text: String) -> String {
        let count = text.count
        return count > maxCharacters
            ? "\(count) caractères : seuls les \(maxCharacters) premiers seront lus"
            : "\(count) caractères sur \(maxCharacters) au plus"
    }

    init(backend: VoiceLibraryBackend, sampleText: String, speed: Float,
         macVoices: @escaping () -> [VoiceSection.MacVoice] = VoiceSection.installedMacVoices,
         announce: ((String) -> Void)? = nil) {
        self.backend = backend
        self.macVoices = macVoices
        errorNote = ErrorNote(announce: announce)
        text = sampleText
        self.speed = min(max(speed, Self.speedRange.lowerBound), Self.speedRange.upperBound)
    }

    var visible: [VoiceLibraryEntry] { VoiceLibrary.filtered(entries, filter) }

    /// La ligne choisie, si les filtres la montrent : une ligne cachée n'est
    /// ni écoutée ni utilisée.
    var selected: VoiceLibraryEntry? { visible.first { $0.id == selection } }

    /// Lit les voix, et choisit celle qui lit — aucune si aucune n'est
    /// cochée : « Utiliser » ne change alors rien sans qu'on ait choisi.
    func load() async {
        let voices = await backend.loadVoiceLibrary()
        entries = VoiceLibrary.entries(voices, macVoices: macVoices())
        if selection == nil { selection = entries.first(where: \.active)?.id }
        loaded = true
    }

    /// « Écouter » une ligne, ou « Arrêter » celle qui sonne.
    func toggle(_ entry: VoiceLibraryEntry) async {
        if playing == entry.id {
            await stop()
            return
        }
        errorNote.clear()
        playing = entry.id
        requested = true
        do {
            try await backend.previewLibraryVoice(model: entry.modelID, voice: entry.voiceID, text: text,
                                                  speed: speed)
            // La demande a abouti : le moteur se prépare déjà (l'instantané
            // est relu au retour). Le prochain repos dit la fin — même si
            // l'aperçu précédent occupait le moteur sans interruption.
            seenActive = true
        } catch {
            playing = nil
            seenActive = false
            errorNote.fail(SettingsStore.message(of: error))
        }
    }

    func stop() async {
        playing = nil
        seenActive = false
        await backend.stopPreview()
    }

    /// L'aperçu du moteur se prépare ou sonne (`active`) : vu en route puis
    /// au repos, il est fini.
    func previewChanged(active: Bool) {
        if active {
            seenActive = true
        } else if seenActive {
            seenActive = false
            playing = nil
        }
    }

    /// La feuille se ferme : l'aperçu se tait, le modèle chargé pour lui part.
    func close() async {
        playing = nil
        await backend.stopPreview()
        await backend.releaseVoicePreview()
    }
}

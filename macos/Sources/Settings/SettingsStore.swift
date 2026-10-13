import SwiftUI

/// Ce que les onglets demandent au moteur. `ReaderModel` le fournit ; les
/// tests le simulent sans toucher au disque.
@MainActor
protocol SettingsBackend: AnyObject {
    func loadSettings() async -> SettingsView
    func loadCapabilities() async -> ModelCapabilities
    func updateSettings(_ patch: SettingsPatch) async throws -> SettingsView
    /// La phrase d'exemple dans une langue (Kokoro : une voix par langue).
    func previewVoice(in language: String) async throws
    func stopPreview() async
}

extension ReaderModel: SettingsBackend {}

/// Les réglages tels que les onglets les montrent et les changent.
///
/// Un seul magasin pour les six onglets. Un changement s'affiche tout de
/// suite, part en patch (le seul champ touché) après une courte pause, et la
/// valeur réellement appliquée — bornes comprises — revient à l'écran.
@MainActor
@Observable
final class SettingsStore {
    @ObservationIgnored private let backend: SettingsBackend
    @ObservationIgnored private var committer: Committer<SettingsView>!
    @ObservationIgnored private var dragging = false
    /// Un `reload` est en cours : les changements faits entre-temps sont mis
    /// de côté et rejoués sur ce qui vient d'être relu, sans rien écrire d'ancien.
    @ObservationIgnored private var reloading = false
    @ObservationIgnored private var queuedEdits: [(inout SettingsView) -> Void] = []
    @ObservationIgnored private var reloadGeneration = 0

    private(set) var values = SettingsView.defaults
    private(set) var capabilities = ModelCapabilities(family: "", voices: [], languages: [],
                                                      honoured: [], requiresReference: false)
    private(set) var loaded = false
    /// Afficher aussi les réglages que le modèle actif ignore.
    var showIgnored = false
    /// La dernière écriture refusée ; le réglage reste affiché.
    var error: String? { writeError.text }
    /// Le refus du dernier « Écouter ».
    var previewNote: String? { previewError.text }
    @ObservationIgnored private let writeError: ErrorNote
    @ObservationIgnored private let previewError: ErrorNote

    @ObservationIgnored private let engineDelay: Duration
    @ObservationIgnored private let dragTimeout: Duration
    @ObservationIgnored private var dragWatchdog: Task<Void, Never>?

    /// `engineDelay` : la pause des réglages qui rechargent la voix (moteur,
    /// calcul, fils, sessions, phrases d'avance) ; une rafale de pressions
    /// ne recharge qu'une fois. `dragTimeout` : un geste sans relâchement
    /// au bout de ce temps est abandonné.
    /// `announce` : ce qui dit un refus à VoiceOver (VoiceOver lui-même par défaut).
    init(backend: SettingsBackend, delay: Duration = .milliseconds(150),
         engineDelay: Duration = .milliseconds(800), dragTimeout: Duration = .seconds(3),
         announce: ((String) -> Void)? = nil) {
        self.backend = backend
        writeError = ErrorNote(announce: announce)
        previewError = ErrorNote(announce: announce)
        self.engineDelay = engineDelay
        self.dragTimeout = dragTimeout
        committer = Committer(delay: delay, writeApplied: { [backend] new, old in
            let patches = SettingsDiff.patches(from: old, to: new)
            if patches.isEmpty { return new }   // rien à écrire
            var applied = new
            for patch in patches { applied = try await backend.updateSettings(patch) }
            return applied
        }, onApplied: { [weak self] sent, applied in
            // Une valeur changée depuis l'envoi reste : elle partira à son tour.
            guard let self, self.values == sent else { return }
            self.values = applied
        }, onResult: { [weak self] error in
            guard let self else { return }
            if let error { self.writeError.fail(SettingsStore.message(of: error)) } else { self.writeError.clear() }
        })
    }

    /// Réglages et capacités, lus hors du fil principal. Rien n'est écrit.
    func load() async {
        let loadedValues = await backend.loadSettings()
        capabilities = await backend.loadCapabilities()
        committer.prime(loadedValues)
        values = loadedValues
        loaded = true
    }

    func reloadCapabilities() async {
        capabilities = await backend.loadCapabilities()
    }

    /// Relit réglages ET capacités : après « Utiliser », après un
    /// téléchargement qui active un modèle, après tout changement de modèle
    /// actif (la voix, la langue et les réglages honorés peuvent avoir changé).
    /// Ce qui attend encore sa pause part d'abord, pour ne pas être perdu.
    func reload() async {
        reloadGeneration += 1
        let generation = reloadGeneration
        reloading = true
        await committer.flush()
        let fresh = await backend.loadSettings()
        let freshCapabilities = await backend.loadCapabilities()
        // Un `reload` plus récent a pris la suite : lui seul s'applique.
        guard generation == reloadGeneration else { return }
        capabilities = freshCapabilities
        committer.prime(fresh)
        dragWatchdog?.cancel()
        dragging = false
        values = fresh
        loaded = true
        reloading = false
        let edits = queuedEdits
        queuedEdits = []
        // Les changements faits pendant la relecture reviennent par-dessus.
        for edit in edits { edit(&values) }
        if !edits.isEmpty { committer.change(values) }
    }

    /// Écrit tout de suite ce qui attend sa pause (fermeture de la fenêtre,
    /// fin de l'application).
    func flush() async { await committer.flush() }

    /// Change un réglage : l'écran suit tout de suite, l'écriture après la pause.
    /// `reloadsVoice` : le réglage recharge la voix, l'écriture attend une
    /// pause plus longue pour qu'une rafale de pressions n'en fasse qu'une.
    func set<T>(_ key: WritableKeyPath<SettingsView, T>, _ value: T, reloadsVoice: Bool = false) {
        values[keyPath: key] = value
        if reloading { queuedEdits.append { $0[keyPath: key] = value }; return }
        if dragging { armWatchdog() }   // le geste vit encore
        guard loaded, !dragging else { return }
        committer.change(values, after: reloadsVoice ? engineDelay : nil)
    }

    /// Vitesse et volume changés depuis le panneau (touches + - 1-4, ⌥↑↓) :
    /// l'écran les reprend et la référence du `Committer` aussi, pour que
    /// revenir à l'ancienne valeur écrive. Un geste, une relecture ou une
    /// écriture en attente l'emportent : leur valeur est la plus récente.
    func followReader(speed: Float, volume: Float) {
        guard loaded, !dragging, !reloading, committer.isIdle else { return }
        guard values.speed != speed || values.volume != volume else { return }
        values.speed = speed
        values.volume = volume
        committer.amendSaved { $0.speed = speed; $0.volume = volume }
    }

    /// Un réglage qui recharge la voix : jamais d'écriture pendant un geste.
    func engineBinding<T>(_ key: WritableKeyPath<SettingsView, T>) -> Binding<T> {
        Binding(get: { self.values[keyPath: key] }, set: { self.set(key, $0, reloadsVoice: true) })
    }

    func binding<T>(_ key: WritableKeyPath<SettingsView, T>) -> Binding<T> {
        Binding(get: { self.values[keyPath: key] }, set: { self.set(key, $0) })
    }

    /// Pour TOUT curseur qui relance la synthèse (vitesse, hauteur, énergie,
    /// échantillonnage) : `onEditingChanged` l'appelle avec `true` au début du
    /// geste et `false` au relâchement. Rien ne part avant le relâchement, puis
    /// une seule écriture tout de suite.
    func dragChanged(_ editing: Bool) {
        dragWatchdog?.cancel()
        dragWatchdog = nil
        dragging = editing
        if editing {
            armWatchdog()
        } else if loaded, !reloading {
            committer.change(values, immediate: true)
        }
    }

    /// Un « relâché » qui ne vient jamais ne doit pas figer toutes les
    /// écritures : sans aucun mouvement pendant `dragTimeout`, le geste prend fin.
    private func armWatchdog() {
        dragWatchdog?.cancel()
        dragWatchdog = Task { [weak self, dragTimeout] in
            try? await Task.sleep(for: dragTimeout)
            guard !Task.isCancelled else { return }
            self?.dragChanged(false)
        }
    }

    /// Un geste encore marqué en cours (onglet quitté, fenêtre fermée) prend fin.
    func endDragging() {
        if dragging { dragChanged(false) }
    }

    var isDragging: Bool { dragging }

    func isIgnored(_ key: SettingKey) -> Bool { key.isIgnored(honoured: capabilities.honoured) }

    func isShown(_ key: SettingKey) -> Bool { showIgnored || !isIgnored(key) }

    /// La voix d'une langue (Kokoro) : l'écran suit, l'écriture après la pause.
    func setVoice(_ voice: String, language: String) {
        var voices = values.voicesByLanguage.filter { $0.language != language }
        voices.append(LanguageVoice(language: language, voice: voice))
        set(\.voicesByLanguage, voices)
    }

    /// La phrase d'exemple dans `language`, avec ce qui vient d'être choisi.
    func listen(language: String) async {
        previewError.clear()
        await committer.flush()
        do { try await backend.previewVoice(in: language) } catch { previewError.fail(Self.message(of: error)) }
    }

    func stopListening() async { await backend.stopPreview() }

    /// Le message d'une erreur du moteur, sans l'habillage de l'enum.
    static func message(of error: Error) -> String {
        if case SillonError.Failed(let message) = error { return message }
        return error.localizedDescription
    }
}

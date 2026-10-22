import Foundation
import KokoroMLX
import Observation

/// Le moteur d'un essai du micro, et ce qu'il sait mesurer en plus.
struct TrialEngine {
    let engine: DictationEngine
    /// Un modèle MLX : le charge avant d'ouvrir le micro et rend la mémoire
    /// qu'il occupe ; `nil` pour la dictée de macOS.
    var load: (() async throws -> TrialMemory?)?
    /// Le temps de calcul cumulé du modèle, en secondes (`SpeechModelRunner`) ;
    /// `nil` : le moteur ne le dit pas (la dictée de macOS).
    var computeSeconds: (() -> Double)?
    /// À la fin de l'essai (fini, annulé, échoué) : libère le modèle essayé
    /// s'il n'est pas celui de la dictée.
    var release: (() -> Void)?
}

/// L'essai du micro (section Dictée des réglages) : une phrase à lire, le
/// niveau du micro, le texte reconnu en direct, puis la précision, les
/// délais, le débit et la mémoire du modèle essayé.
///
/// L'essai s'arrête seul (`stop()`, comme « Arrêter ») dès que la phrase est
/// reconnue mot pour mot, ou après 1,5 s de silence qui suivent la parole
/// (`SpeechEndDetector`), une fois du texte reçu.
///
/// Le texte n'est jamais saisi nulle part ; le modèle de la dictée
/// (`dictationModel`) ne change pas. L'essai et la dictée ne tiennent jamais
/// le micro ensemble : l'essai refuse de partir pendant une dictée
/// (`dictationActive`), et la dictée refuse pendant un essai
/// (`DictationController.busy`). Le modèle passe par le même coureur que la
/// dictée (`SpeechModelRunner`, sous `MLXGate`) : jamais deux en mémoire.
@MainActor
@Observable
final class MicTrialController {
    enum Phase: Equatable {
        case idle
        /// Le modèle se charge, ou le micro s'ouvre : ce qui est dit.
        case preparing(String)
        case listening
        /// « Arrêter » : le texte final arrive.
        case finishing
        case failed(String)
    }

    nonisolated static let dictationBusy = String(localized: "A dictation is running: the trial can start when it’s done.")
    nonisolated static let trialBusy = String(localized: "A microphone trial is running in settings: stop it to dictate.")
    nonisolated static let noFinal = String(localized: "The trial didn’t complete: no final text arrived (nothing was heard, or the model didn’t respond). The previous result is kept.")

    private(set) var phase: Phase = .idle
    private(set) var transcript = Transcript()
    /// La phrase à lire.
    private(set) var sentence: String
    /// Le début de la parole a été repéré.
    private(set) var speechHeard = false
    /// Le modèle de l'essai en cours, ou du dernier.
    private(set) var modelID: String?
    /// Le dernier essai de chaque modèle (`DictationModel` : « system » ou
    /// l'id du paquet).
    private(set) var results: [String: TrialResult]
    /// Dit à VoiceOver à la fin d'un essai.
    private(set) var announcement: String?
    /// Texte libre : pas de phrase, pas de précision.
    var freeText = false
    /// Le spectre du micro, lu par le dessin du niveau à chaque image.
    @ObservationIgnored let spectrum = SpectrumFeed()
    /// Juste avant d'ouvrir le micro : la lecture se met en pause.
    @ObservationIgnored var willStart: @MainActor () -> Void = {}

    @ObservationIgnored private let make: @MainActor (String) -> TrialEngine?
    @ObservationIgnored private let store: TrialResultsStore
    @ObservationIgnored private let currentLanguage: () -> DictationLanguage
    @ObservationIgnored private let dictationActive: @MainActor () -> Bool
    @ObservationIgnored private let clock: () -> Double
    @ObservationIgnored private var active: TrialEngine?
    @ObservationIgnored private var timeline = TrialTimeline()
    @ObservationIgnored private var onset = SpeechOnsetDetector()
    @ObservationIgnored private var end = SpeechEndDetector()
    @ObservationIgnored private var memory: TrialMemory?
    @ObservationIgnored private var computeBefore: Double?
    /// Un texte final est arrivé : sans lui, pas de score.
    @ObservationIgnored private var gotFinal = false
    /// Les phrases lues jusqu'ici ; la phrase montrée a-t-elle déjà servi ?
    @ObservationIgnored private var trials = 0
    @ObservationIgnored private var sentenceUsed = false
    @ObservationIgnored private var language: DictationLanguage
    /// Chaque essai a son numéro : ce qui arrive d'un essai fini est ignoré.
    @ObservationIgnored private var session = 0

    init(make: @escaping @MainActor (String) -> TrialEngine?,
         store: TrialResultsStore = TrialResultsStore(),
         language: @escaping () -> DictationLanguage = { DictationLanguage.stored() },
         dictationActive: @escaping @MainActor () -> Bool,
         clock: @escaping () -> Double = MicTrialController.monotonicSeconds) {
        self.make = make
        self.store = store
        self.currentLanguage = language
        self.dictationActive = dictationActive
        self.clock = clock
        let language = language()
        self.language = language
        sentence = TrialSentences.sentence(Self.code(language), trial: 0)
        results = store.all()
    }

    var isRunning: Bool {
        switch phase {
        case .preparing, .listening, .finishing: true
        case .idle, .failed: false
        }
    }

    /// La langue des réglages a pu changer : la phrase la suit (hors essai).
    func refreshLanguage() {
        guard !isRunning else { return }
        let now = currentLanguage()
        guard now != language else { return }
        language = now
        sentence = TrialSentences.sentence(Self.code(now), trial: trials)
        sentenceUsed = false
    }

    /// « Une autre phrase ».
    func nextSentence() {
        guard !isRunning else { return }
        trials += 1
        sentence = TrialSentences.sentence(Self.code(language), trial: trials)
        sentenceUsed = false
    }

    /// « Démarrer l'essai » : `id` est un `DictationModel` (« system » ou un
    /// paquet installé).
    func start(model id: String) {
        guard !isRunning else { return }
        guard !dictationActive() else {
            phase = .failed(Self.dictationBusy)
            return
        }
        guard let trial = make(id) else {
            phase = .failed(String(localized: "\(DictationModel.label(id)) isn’t installed: the trial can’t start."))
            return
        }
        refreshLanguage()
        if sentenceUsed { nextSentence() }
        sentenceUsed = true
        session &+= 1
        let mine = session
        willStart()
        modelID = id
        active = trial
        transcript = Transcript()
        timeline = TrialTimeline()
        onset = SpeechOnsetDetector()
        end = SpeechEndDetector()
        speechHeard = false
        memory = nil
        computeBefore = nil
        gotFinal = false
        announcement = nil
        spectrum.reset()
        phase = .preparing(trial.load == nil ? String(localized: "Opening the microphone…") : String(localized: "Loading the model…"))
        let locale = language.locale
        Task {
            do {
                if let load = trial.load {
                    let measured = try await load()
                    guard session == mine else { return }
                    memory = measured
                    phase = .preparing(String(localized: "Opening the microphone…"))
                }
                computeBefore = trial.computeSeconds?()
                try await trial.engine.start(locale: locale) { event in
                    guard self.session == mine else { return }
                    self.receive(event)
                }
            } catch {
                guard session == mine else { return }
                session &+= 1
                active = nil
                await trial.engine.cancel()
                trial.release?()
                let message = (error as? DictationError)?.message
                    ?? DictationError.failed(error.localizedDescription).message
                phase = .failed(message)
            }
        }
    }

    /// « Arrêter » : le texte final, puis les résultats.
    func stop() {
        switch phase {
        case .preparing: cancel()
        case .listening: break
        default: return
        }
        guard phase == .listening, let trial = active else { return }
        let mine = session
        timeline.stop = clock()
        phase = .finishing
        Task {
            await trial.engine.finish()
            guard session == mine else { return }
            timeline.final = clock()
            finish(trial)
        }
    }

    /// La fenêtre se ferme, ou la section change : rien n'est gardé.
    func cancel() {
        guard isRunning, let trial = active else {
            if case .failed = phase { phase = .idle }
            return
        }
        session &+= 1
        active = nil
        phase = .idle
        Task {
            await trial.engine.cancel()
            trial.release?()
        }
    }

    /// Le modèle est supprimé : ses résultats aussi.
    func forget(_ id: String) {
        store.clear(id)
        results[id] = nil
    }

    private func receive(_ event: DictationEvent) {
        switch event {
        case .downloading(let fraction):
            if case .preparing = phase {
                phase = .preparing(String(localized: "The language is installing: \(Int((fraction * 100).rounded()))%"))
            }
        case .listening:
            if case .preparing = phase {
                phase = .listening
                timeline.listening = clock()
            }
        case .text(let text, let isFinal):
            transcript.receive(text, isFinal: isFinal)
            if isFinal { gotFinal = true }
            if timeline.firstText == nil, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                timeline.firstText = clock()
            }
            // La phrase est dite en entier : inutile d'attendre « Arrêter ».
            if phase == .listening, !freeText, TrialScore.isComplete(reference: sentence, heard: transcript.text) {
                stop()
            }
        case .spectrum(let columns):
            spectrum.push(columns)
            if phase == .listening, onset.push(columns) {
                timeline.speechOnset = clock()
                speechHeard = true
            }
            // La parole s'est tue, et du texte est arrivé : l'essai s'arrête.
            if phase == .listening, speechHeard, end.push(columns), !transcript.isEmpty {
                stop()
            }
        case .latency, .memory, .loading, .loaded:
            // L'essai mesure ses délais et sa mémoire lui-même
            // (`TrialTimeline`, `TrialMeasure`).
            break
        }
    }

    private func finish(_ trial: TrialEngine) {
        active = nil
        defer { trial.release?() }
        guard let id = modelID else { phase = .idle; return }
        guard gotFinal else {
            phase = .failed(Self.noFinal)
            announcement = Self.noFinal
            return
        }
        let heard = transcript.text
        let speed: Double? = {
            guard let now = trial.computeSeconds?(), let before = computeBefore,
                  let audio = timeline.audioSeconds else { return nil }
            return TrialTimeline.speed(audioSeconds: audio, computeSeconds: now - before)
        }()
        // Le modèle était déjà chargé : la mémoire mesurée d'un essai
        // précédent vaut mieux que l'estimation.
        var memory = memory
        if memory?.measured == false, let previous = results[id], previous.memoryMeasured,
           let bytes = previous.memoryBytes {
            memory = TrialMemory(bytes: bytes, measured: true)
        }
        let first = timeline.firstTextDelay
        let result = TrialResult(
            date: Date(), language: Self.code(language), freeText: freeText,
            precision: freeText ? nil : TrialScore.precision(reference: sentence, heard: heard),
            firstDelay: first?.seconds, firstDelayFromSpeech: first?.basis == .speech,
            finalDelay: timeline.finalDelay, speed: speed,
            memoryBytes: memory?.bytes, memoryMeasured: memory?.measured ?? false)
        store.save(result, for: id)
        results[id] = result
        announcement = TrialFormat.announcement(result, model: DictationModel.label(id),
                                                size: { SizeFormat.bytes($0) })
        phase = .idle
    }

    static func code(_ language: DictationLanguage) -> String {
        MLXDictationEngine.languageCode(language.locale)
    }

    nonisolated static func monotonicSeconds() -> Double {
        let d = ContinuousClock.now - origin
        return Double(d.components.seconds) + Double(d.components.attoseconds) / 1e18
    }

    nonisolated private static let origin = ContinuousClock.now
}

// MARK: - Les moteurs de l'essai

extension Dictation {
    /// Le moteur d'un essai pour `id` (« system » ou un paquet installé,
    /// `directory`), avec le coureur partagé de la dictée (`runner`) : un
    /// seul modèle en mémoire, sous `MLXGate`, plafonds par `MLXBudget`.
    @MainActor
    static func makeTrialEngine(id: String, directory: URL?, runner: SpeechModelRunner?) -> TrialEngine? {
        if id == DictationModel.system {
            #if compiler(>=6.2)
            if #available(macOS 26, *) { return TrialEngine(engine: SystemDictationEngine()) }
            #endif
            return nil
        }
        guard let directory, let runner,
              let engine = makeModelEngine(id: id, directory: directory, runner: runner) else { return nil }
        // Le modèle de la dictée reste chargé pour elle ; un autre part dès
        // la fin de l'essai (hors du fil principal : la libération attend
        // le verrou), sans toucher à celui qu'une dictée aurait chargé depuis.
        let release: () -> Void = {
            guard id != DictationModel.stored() else { return }
            Task.detached(priority: .utility) { runner.release(ifLoaded: directory) }
        }
        return TrialEngine(engine: engine,
                           load: { try await TrialMeasure.load(runner: runner, directory: directory) },
                           computeSeconds: {
                               let d = runner.computeTime
                               return Double(d.components.seconds) + Double(d.components.attoseconds) / 1e18
                           },
                           release: release)
    }
}

/// La mémoire d'un modèle de dictée : la mémoire active de MLX avant et
/// après son chargement, lue sous `MLXGate` (aucun calcul ne s'y glisse).
/// Un autre modèle de dictée chargé est d'abord libéré (sinon la différence
/// mêlerait les deux) ; le même déjà chargé ne se mesure pas : le poids de
/// ses fichiers en tient lieu.
enum TrialMeasure {
    static func load(runner: SpeechModelRunner, directory: URL) async throws -> TrialMemory? {
        try await Task.detached(priority: .userInitiated) {
            let loaded = runner.loadedDirectory
            var before: Int?
            if loaded != directory {
                if loaded != nil { runner.release() }
                before = MLXGate.shared.run { DictationMLX.counters().active }
            }
            try await runner.prepare(directory: directory)
            let after = before == nil ? nil : MLXGate.shared.run { DictationMLX.counters().active }
            return TrialMemory.reading(before: before, after: after, estimate: MLXBudget.weightBytes(directory))
        }.value
    }
}

import AppKit
import Observation

/// L'application où le texte sera saisi : celle qui a le clavier.
struct DictationTarget: Equatable {
    let processIdentifier: pid_t
    let name: String
    let icon: NSImage?

    static func == (a: Self, b: Self) -> Bool { a.processIdentifier == b.processIdentifier }

    /// L'application au premier plan.
    @MainActor
    static func frontmost() -> DictationTarget? {
        guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
        return DictationTarget(processIdentifier: app.processIdentifier,
                               name: app.localizedName ?? "", icon: app.icon)
    }
}

/// Montre ou retire la fenêtre de la dictée.
@MainActor
protocol DictationPresenter: AnyObject {
    func show()
    func hide()
}

/// La dictée, d'un seul raccourci : un appui ouvre la fenêtre et le micro,
/// le texte s'y écrit pendant qu'on parle ; un second appui le saisit dans
/// le champ qui a le focus. Échap annule.
@MainActor
@Observable
final class DictationController {
    enum Phase: Equatable {
        /// Fenêtre retirée.
        case hidden
        /// Le micro s'ouvre ; `download` : la langue s'installe (0 à 1).
        case preparing(download: Double?)
        /// On parle, le texte s'écrit.
        case listening
        /// Les derniers mots arrivent, puis la saisie.
        case finishing
        /// Un mot passager (« Texte copié… »), puis la fenêtre se retire.
        case notice(String)
        case failed(String)
    }

    private(set) var phase: Phase = .hidden
    private(set) var transcript = Transcript()
    /// Depuis quand le micro écoute (le chronomètre de la fenêtre).
    private(set) var listeningSince: Date?
    /// Le spectrogramme : lu par le dessin à chaque image, hors observation.
    @ObservationIgnored let spectrum = SpectrumFeed()
    private(set) var language: DictationLanguage = .french
    /// Le texte de la dernière dictée (saisie ou annulée) : le menu le recopie.
    private(set) var lastText: String?
    /// Là où le texte ira : l'icône de l'application, dans la pilule.
    private(set) var target: DictationTarget?
    /// L'autorisation qui a manqué : la fenêtre propose de la demander.
    private(set) var missingPermission: Permission?

    @ObservationIgnored private let engine: DictationEngine
    @ObservationIgnored private let insert: @MainActor (String) async -> InsertOutcome
    @ObservationIgnored private let currentLanguage: () -> DictationLanguage
    @ObservationIgnored private let currentTarget: @MainActor () -> DictationTarget?
    /// Où « Copier la dernière dictée » pose le texte.
    @ObservationIgnored var copy: @MainActor (String) -> Void = { text in
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
    @ObservationIgnored weak var presenter: DictationPresenter?
    /// Juste avant d'ouvrir le micro : la lecture se met en pause (le micro
    /// entendrait la voix).
    @ObservationIgnored var willStart: @MainActor () -> Void = {}
    /// Le micro est déjà pris (un essai du micro, dans les réglages) : la
    /// raison, dite à la place de la dictée ; `nil` : libre.
    @ObservationIgnored var busy: @MainActor () -> String? = { nil }
    /// Durée d'un mot passager ; une erreur reste plus longtemps.
    @ObservationIgnored var noticeDuration: Duration = .milliseconds(1800)
    @ObservationIgnored var failureDuration: Duration = .seconds(4)
    /// Chaque dictée a son numéro : ce qui arrive d'une dictée finie est ignoré.
    @ObservationIgnored private var session = 0

    init(engine: DictationEngine,
         insert: @escaping @MainActor (String) async -> InsertOutcome,
         language: @escaping () -> DictationLanguage = { DictationLanguage.stored() },
         target: @escaping @MainActor () -> DictationTarget? = { DictationTarget.frontmost() }) {
        self.engine = engine
        self.insert = insert
        self.currentLanguage = language
        self.currentTarget = target
    }

    /// Une autre application est passée au premier plan pendant la dictée :
    /// c'est là que le texte ira.
    func targetMayHaveChanged() {
        guard isActive else { return }
        let next = currentTarget()
        if next != target { target = next }
    }

    var isActive: Bool { phase != .hidden }

    /// La dictée tient le micro (ou va le tenir) : un message encore
    /// affiché (échec, avis) ne compte pas.
    var holdsMicrophone: Bool {
        switch phase {
        case .preparing, .listening, .finishing: true
        case .hidden, .notice, .failed: false
        }
    }

    /// Le raccourci : ouvre la dictée, ou saisit ce qui a été dit.
    func toggle() {
        switch phase {
        case .hidden, .notice, .failed:
            if let reason = busy() {
                session &+= 1
                flash(.failed(reason), for: failureDuration, session: session)
            } else {
                start()
            }
        case .preparing: cancel()
        case .listening: commit()
        case .finishing: break
        }
    }

    /// « Autoriser » : la question de macOS, ou le volet des Réglages
    /// Système. La fenêtre se retire pour ne pas les recouvrir.
    func requestMissingPermission() {
        guard let permission = missingPermission else { return }
        cancel()
        Task { await permission.request() }
    }

    /// Échap, « Annuler » : rien n'est saisi.
    func cancel() {
        guard phase != .hidden else { return }
        remember(transcript.text)
        session &+= 1
        hide()
        Task { await engine.cancel() }
    }

    private func start() {
        session &+= 1
        let id = session
        willStart()
        transcript = Transcript()
        listeningSince = nil
        spectrum.reset()
        language = currentLanguage()
        missingPermission = nil
        target = currentTarget()
        phase = .preparing(download: nil)
        presenter?.show()
        Task {
            do {
                // Le contrôleur vit autant que l'app : le moteur peut le garder.
                try await engine.start(locale: language.locale) { event in
                    guard self.session == id else { return }
                    self.receive(event)
                }
            } catch {
                guard session == id else { return }
                await engine.cancel()
                if (error as? DictationError) == .microphoneDenied { missingPermission = .microphone }
                let message = (error as? DictationError)?.message
                    ?? DictationError.failed(error.localizedDescription).message
                flash(.failed(message), for: failureDuration, session: id)
            }
        }
    }

    private func receive(_ event: DictationEvent) {
        switch event {
        case .downloading(let fraction):
            if case .preparing = phase { phase = .preparing(download: fraction) }
        case .listening:
            if case .preparing = phase {
                phase = .listening
                listeningSince = Date()
            }
        case .text(let text, let isFinal):
            transcript.receive(text, isFinal: isFinal)
        case .spectrum(let columns):
            spectrum.push(columns)
        }
    }

    /// Les derniers mots, puis la saisie. La fenêtre se retire avant le ⌘V :
    /// elle ne prend jamais le clavier, mais on voit le texte arriver une
    /// fois qu'elle est partie.
    /// La fermeture de la fenêtre (0,05 + 0,34 s) : la saisie attend qu'elle soit finie.
    static let closingDuration: Duration = .milliseconds(400)
    @ObservationIgnored var closing: Duration = DictationController.closingDuration

    private func commit() {
        let id = session
        phase = .finishing
        // La fenêtre part d'abord ; les derniers mots arrivent pendant sa
        // sortie, et le texte n'est collé qu'une fois elle partie.
        presenter?.hide()
        Task {
            async let closed: Void? = try? Task.sleep(for: closing)
            await engine.finish()
            _ = await closed
            // Le démarrage a échoué pendant ce temps (modèle qui ne se charge
            // pas) : son message reste, rien n'est saisi.
            guard session == id, phase == .finishing else { return }
            let text = transcript.text
            remember(text)
            switch await insert(text) {
            case .pasted, .nothing:
                guard session == id else { return }
                hide()
            case .copied(let reason):
                guard session == id else { return }
                if reason == .noAccessibility { missingPermission = .accessibility }
                flash(.notice(reason.message),
                      for: missingPermission == nil ? noticeDuration : failureDuration, session: id)
            }
        }
    }

    /// Un état montré un moment, puis la fenêtre se retire (sauf si une
    /// autre dictée a commencé entre-temps).
    private func flash(_ shown: Phase, for duration: Duration, session id: Int) {
        phase = shown
        presenter?.show()
        Task {
            try? await Task.sleep(for: duration)
            guard session == id, phase == shown else { return }
            hide()
        }
    }

    /// « Copier la dernière dictée ».
    func copyLast() {
        guard let lastText else { return }
        copy(lastText)
    }

    private func remember(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !text.isEmpty { lastText = text }
    }

    private func hide() {
        phase = .hidden
        presenter?.hide()
    }
}

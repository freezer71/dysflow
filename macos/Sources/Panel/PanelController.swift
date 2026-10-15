import AppKit
import SwiftUI

/// Montre la fenêtre sous l'une de ses deux formes — le lecteur, le mini —
/// ou la retire, et exécute les actions du clavier. Une seule fenêtre :
/// chaque forme change son contenu et son cadre. Fermer met la lecture en
/// pause. Le placement (`PanelPlacement`), le clavier (`PanelKeyRouter`) et
/// VoiceOver (`PanelAnnouncements`) vivent à part.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Forme courante, dernière taille, place du mini et cadre du lecteur,
    /// retenus d'un lancement à l'autre.
    @ObservationIgnored let store: DisplayModeStore
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var placement: PanelPlacement!
    @ObservationIgnored private var keys: PanelKeyRouter!
    @ObservationIgnored private var announcements: PanelAnnouncements!
    /// ↑ ↓ : la vue de lecture affichée (ou sa copie cachée du mini) dit
    /// où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()

    private(set) var mode: DisplayMode = .hidden
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture et à chaque réglage écrit.
    var reading = ReadingDisplay()
    /// Le sommaire, fermé à l'ouverture : le bouton ou « t » l'ouvre.
    var sidebarVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false
    /// Le mode édition : le texte du lecteur devient modifiable.
    private(set) var editing = false
    /// Le texte en cours de modification.
    var draft = ""
    /// Le texte au début de la modification : inchangé, rien ne se recharge.
    @ObservationIgnored private var draftOrigin = ""
    @ObservationIgnored private var resumeAfterEditing = false

    /// Où va ⌘C : le presse-papiers général ; les tests en donnent un à eux.
    @ObservationIgnored var pasteboard = NSPasteboard.general
    /// ⌘V : l'app lit le presse-papiers (texte ou image).
    @ObservationIgnored private let readClipboard: @MainActor () -> Void
    /// ⌘O : l'app montre le panneau d'ouverture.
    @ObservationIgnored private let chooseFile: @MainActor () -> Void
    /// Un fichier déposé : l'app l'ouvre.
    @ObservationIgnored private let openFile: @MainActor (URL) -> Void

    /// Ouvre la fenêtre des réglages (un onglet, ou le dernier ouvert) :
    /// `,` au clavier, l'écrou de la barre.
    @ObservationIgnored let openSettings: @MainActor (SettingsTab?) -> Void
    /// La même, pour l'environnement des vues : une seule valeur, posée une
    /// fois, pour que SwiftUI n'y voie jamais de changement.
    @ObservationIgnored let openSettingsAction: OpenSettingsTabAction

    init(model: ReaderModel, store: DisplayModeStore? = nil,
         openSettings: @escaping @MainActor (SettingsTab?) -> Void = { _ in },
         readClipboard: @escaping @MainActor () -> Void = {},
         chooseFile: @escaping @MainActor () -> Void = {},
         openFile: @escaping @MainActor (URL) -> Void = { _ in }) {
        self.model = model
        self.openSettings = openSettings
        self.readClipboard = readClipboard
        self.chooseFile = chooseFile
        self.openFile = openFile
        openSettingsAction = OpenSettingsTabAction(open: openSettings)
        let store = store ?? DisplayModeStore(defaults: Self.processDefaults)
        // Un lancement ne montre rien ; la dernière taille reste pour la suite.
        store.state = store.state.relaunched
        self.store = store
        // Avant la vue : son corps lit le placement (largeur du texte).
        placement = PanelPlacement(window: window, store: store, mode: { [unowned self] in self.mode })
        keys = PanelKeyRouter(window: window,
                              isEditing: { [unowned self] in self.editing },
                              action: { [unowned self] in self.action(for: $0) },
                              finishEditing: { [unowned self] in self.finishEditing() },
                              perform: { [unowned self] in self.perform($0) })
        announcements = PanelAnnouncements(window: window)
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("lecteur")
        host.setAccessibilityLabel("Lecteur")
        // Toute fermeture (⌘W, close(), performClose:) passe par `close()`.
        let guardian = PanelCloseGuard { [weak self] in self?.close() }
        closeGuard = guardian
        window.delegate = guardian
        observeSettingsWindow()
    }

    /// Les préférences du processus, lues une fois : sous les tests, chaque
    /// appel à `processDefaults()` vide la suite.
    private static let processDefaults = DisplayModeStore.processDefaults()

    /// La largeur du texte du lecteur : la copie cachée du mini la garde,
    /// pour que ↑ ↓ visent les mêmes lignes.
    var shadowTextWidth: CGFloat {
        placement.frame(for: .reader).width - 2 * PanelGeometry.horizontalPadding
    }

    /// Un dépôt sur la fenêtre : le dernier fichier s'ouvre. Faux si rien
    /// n'était un fichier.
    @discardableResult
    func openDropped(_ urls: [URL]) -> Bool {
        guard let file = DroppedFiles.file(from: urls) else { return false }
        openFile(file)
        return true
    }

    // MARK: - Formes

    /// Une transition, retenue puis appliquée. Si la fenêtre disparaît, la
    /// lecture s'arrête.
    func apply(_ event: DisplayEvent) {
        let before = store.state
        let next = before.applying(event)
        store.state = next
        if before.hides(next) { stopForClosing() }
        present(next.current)
    }

    /// ⌃⌥Espace, ⌘V, un fichier, « Afficher le lecteur » : la dernière taille.
    func show() { apply(.show) }
    /// ⌃⌥S.
    func toggleVisibility() { apply(.toggleVisibility) }
    /// ⌘M, « Réduire », « Agrandir ».
    func toggleSize() { apply(.toggleSize) }
    /// ⌘W, Échap, le bouton fermer.
    func close() { apply(.close) }

    /// Fermer arrête la lecture : une modification s'applique sans relancer
    /// la voix, puis la pause. Le document et la position restent.
    private func stopForClosing() {
        if editing { finishEditing(resume: false) }
        model.refresh()
        if model.snapshot.playing { model.send(.pause) }
    }

    private func present(_ target: DisplayMode) {
        guard target.isVisible else { hideWindow(); return }
        // Le mini n'a pas de place pour écrire : la modification s'applique avant.
        if editing, target == .mini { finishEditing() }
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        let wasHidden = mode == .hidden || !window.isVisible
        if wasHidden || target != mode {
            mode = target
            placement.applySizeLimits(for: target)
            // D'une forme à l'autre, le cadre glisse vers sa nouvelle place.
            placement.place(placement.frame(for: target), animated: !wasHidden)
            updateIdentifier()
        }
        window.makeKeyAndOrderFront(nil)
        // La cadence de relecture suit ce qui est montré : le texte (à
        // chaque image en lecture), ou le mini (4 Hz en lecture, rien en pause).
        model.presence = target == .mini ? .bar : .full
        // L'annonceur ne voit rien fenêtre masquée : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcements.reseed(with: model.snapshot)
        keys.install()
    }

    /// Masqué : la fenêtre se retire.
    private func hideWindow() {
        guard mode != .hidden else { return }
        if editing { finishEditing(resume: false) }
        // Une recherche en cours ne survit pas : sinon le lecteur reviendrait
        // avec un champ vide et un compteur « n / m » périmé.
        if searchActive { searchActive = false; model.send(.searchCancel) }
        helpVisible = false
        mode = .hidden
        if window.isVisible { window.orderOut(nil) }
        model.presence = .hidden
        keys.remove()
    }

    // MARK: - Réglages

    /// Relit ce que le panneau affiche des réglages, hors du fil principal :
    /// un changement dans la fenêtre des réglages s'y voit aussitôt.
    /// Des relectures qui se chevauchent : seule la dernière demandée compte.
    func reloadReading() {
        readingGeneration &+= 1
        let generation = readingGeneration
        Task {
            let loaded = ReadingDisplay(await model.loadSettings())
            guard generation == readingGeneration else { return }
            reading = loaded
        }
    }
    @ObservationIgnored private var readingGeneration = 0

    /// Fenêtre des réglages ouverte : le panneau reste affiché à côté, mais
    /// ne la recouvre plus, quel que soit le mode.
    private(set) var settingsVisible = false

    func settingsVisibilityChanged(_ visible: Bool) {
        guard visible != settingsVisible else { return }
        settingsVisible = visible
        window.level = PanelLevel.level(settingsVisible: visible)
    }

    private func observeSettingsWindow() {
        let center = NotificationCenter.default
        settingsObservers = [
            center.addObserver(forName: NSWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] note in
                let window = note.object as? NSWindow
                MainActor.assumeIsolated {
                    guard let self, let w = window, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(true)
                    // Devant le panneau, sans passer au-dessus des autres apps.
                    w.orderFront(nil)
                }
            },
            center.addObserver(forName: NSWindow.willCloseNotification, object: nil, queue: .main) { [weak self] note in
                let window = note.object as? NSWindow
                MainActor.assumeIsolated {
                    guard let self, let w = window, PanelLevel.isSettingsWindow(w) else { return }
                    self.settingsVisibilityChanged(false)
                }
            },
        ]
    }
    @ObservationIgnored private var settingsObservers: [NSObjectProtocol] = []

    // MARK: - Clavier

    func action(for press: KeyPress) -> ReaderAction? { KeyMap.action(for: press) }

    /// La phrase à dire, ou rien : jamais fenêtre masquée.
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func announcementText(for s: Snapshot) -> String? {
        announcements.text(for: s, doc: model.document, hidden: mode == .hidden)
    }

    /// Dit à VoiceOver un changement d'état (lecture, pause, section), jamais
    /// un mot. Sans effet quand VoiceOver est éteint.
    func announce(_ s: Snapshot) {
        guard let text = announcementText(for: s) else { return }
        post(text)
    }

    /// Où part une annonce : VoiceOver, s'il est allumé. Remplaçable, pour
    /// qu'un test compte ce que le vrai chemin dit.
    var post: (String) -> Void {
        get { announcements.post }
        set { announcements.post = newValue }
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .toggleSize: toggleSize()
        case .close: close()
        case .escape:
            // Échap : la saisie se valide, puis l'aide, la recherche et le
            // sommaire se ferment, puis le lecteur.
            if editing { finishEditing() }
            else if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else if sidebarVisible, mode == .reader { sidebarVisible = false }
            else { close() }
        case .copy: copy()
        case .paste: readClipboard()
        case .openFile: chooseFile()
        case .openSearch:
            if mode != .reader { apply(.choose(.reader)) }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode != .reader {
                apply(.choose(.reader))
                sidebarVisible = true
            } else {
                sidebarVisible.toggle()
            }
        case .showHelp:
            // Le mini n'a pas la place : l'aide s'ouvre dans le lecteur.
            if mode == .mini {
                apply(.choose(.reader))
                helpVisible = true
            } else {
                helpVisible.toggle()
            }
        case .openSettings:
            openSettings(nil)
        case .edit:
            beginEditing()
        }
    }

    /// ⌘C, « Copier » : la sélection, sinon la phrase en cours. N'entre
    /// jamais en édition.
    func copy() {
        guard let choice = ReadingCopy.choose(selection: lines.selectedText,
                                              sentence: model.currentSentenceText) else { return }
        pasteboard.clearContents()
        pasteboard.setString(choice.text, forType: .string)
        flash(choice.message)
    }

    /// « Lire à partir d'ici » : la voix rejoint ce mot et lit.
    func readFrom(location: Int) {
        guard let command = lines.readCommand(at: location) else { return }
        model.send(command)
        model.refresh()
        if !model.snapshot.playing { model.send(.togglePlay) }
    }

    // MARK: - Édition

    /// Bouton Modifier ou « e » : la lecture se met en pause et le texte devient
    /// modifiable. Le mini n'a pas la place : le lecteur s'ouvre.
    func beginEditing() {
        guard !editing, model.snapshot.hasDocument else { return }
        resumeAfterEditing = model.snapshot.playing
        if resumeAfterEditing { model.send(.pause) }
        if searchActive { searchActive = false; model.send(.searchCancel) }
        helpVisible = false
        if mode != .reader { apply(.choose(.reader)) }
        draftOrigin = model.editableText
        draft = draftOrigin
        editing = true
    }

    /// Échap, ⌘↩ ou « Terminé » : le texte modifié remplace le document, la
    /// voix reprend à la même phrase. `resume` faux (fermeture) : la voix
    /// reste en pause.
    func finishEditing(resume: Bool = true) {
        guard editing else { return }
        editing = false
        let play = resume && resumeAfterEditing
        if draft != draftOrigin, !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            model.reopen(edited: draft, play: play)
            flash("Texte modifié")
        } else if play {
            model.send(.togglePlay)
        }
        window.makeFirstResponder(nil)
    }

    /// « Annuler » : les modifications sont oubliées.
    func cancelEditing() {
        guard editing else { return }
        draft = draftOrigin
        finishEditing()
    }

    /// La voix reprend au premier mot de la ligne affichée voisine ; rien
    /// au-delà de la première ou de la dernière.
    private func moveLine(_ delta: Int) {
        let s = model.snapshot
        guard s.hasDocument else { return }
        guard let target = lines.target(word: s.word, sentence: s.sentence, delta: delta) else {
            flash("Aucune ligne de ce côté")
            return
        }
        model.send(.seekToWord(sentence: target.sentence, word: target.word))
    }

    /// Un message qui s'efface seul, sauf si un autre l'a remplacé.
    private func flash(_ text: String) {
        model.message = text
        messageToken &+= 1
        let token = messageToken
        DispatchQueue.main.asyncAfter(deadline: .now() + messageDuration) { [weak self] in
            guard let self, self.messageToken == token, self.model.message == text else { return }
            self.model.message = nil
        }
    }
    @ObservationIgnored private var messageToken = 0

    /// L'identifiant pour les tests, le nom pour VoiceOver.
    private func updateIdentifier() {
        let mini = mode == .mini
        window.contentView?.setAccessibilityIdentifier(mini ? "lecteur-mini" : "lecteur")
        window.contentView?.setAccessibilityLabel(mini ? "Lecteur réduit" : "Lecteur")
    }
}

/// Le délégué de la fenêtre : une fermeture demandée passe par `close()`.
@MainActor
private final class PanelCloseGuard: NSObject, NSWindowDelegate {
    let hide: () -> Void
    init(hide: @escaping () -> Void) { self.hide = hide }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        hide()
        return false
    }

    /// `close()` ne demande pas l'avis du délégué : on remet l'état d'accord.
    func windowWillClose(_ notification: Notification) {
        hide()
    }
}

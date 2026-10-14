import AppKit
import SwiftUI

/// Montre la fenêtre dans l'un des quatre modes — petit, moyen, grand,
/// arrière-plan — et exécute les actions du clavier. Une seule fenêtre :
/// chaque mode change son contenu et sa taille ; l'arrière-plan la retire et
/// la lecture continue. Le placement (`PanelPlacement`), le clavier
/// (`PanelKeyRouter`) et VoiceOver (`PanelAnnouncements`) vivent à part.
@MainActor
@Observable
final class PanelController {
    @ObservationIgnored let model: ReaderModel
    @ObservationIgnored let window = ReadingPanel()
    /// Mode courant, dernier mode visible et place du mode petit, retenus
    /// d'un lancement à l'autre.
    @ObservationIgnored let store: DisplayModeStore
    /// Durée d'affichage d'un message passager (« Phrase copiée »).
    @ObservationIgnored var messageDuration: TimeInterval = 2
    @ObservationIgnored private var contentHeight: CGFloat = 0
    @ObservationIgnored private var resizePending = false
    @ObservationIgnored private var closeGuard: PanelCloseGuard?
    @ObservationIgnored private var placement: PanelPlacement!
    @ObservationIgnored private var keys: PanelKeyRouter!
    @ObservationIgnored private var announcements: PanelAnnouncements!
    /// ↑ ↓ : la vue de lecture affichée (ou sa copie cachée du mode petit)
    /// dit où commence la ligne voisine.
    @ObservationIgnored let lines = LineNavigator()

    private(set) var mode: DisplayMode = .background
    /// Réglages de lecture du moteur (interlignage, règle), relus à chaque
    /// ouverture et à chaque réglage écrit.
    var reading = ReadingDisplay()
    var sidebarVisible = true
    var bookmarksVisible = false
    var searchActive = false
    /// Compte les « / » : chacun rend le focus au champ, même déjà ouvert.
    private(set) var openSearchRequests = 0
    var helpVisible = false
    /// Le mode édition : le texte du panneau devient modifiable.
    private(set) var editing = false
    /// Le texte en cours de modification.
    var draft = ""
    /// Le texte au début de la modification : inchangé, rien ne se recharge.
    @ObservationIgnored private var draftOrigin = ""
    @ObservationIgnored private var resumeAfterEditing = false

    /// Ouvre la fenêtre des réglages (un onglet, ou le dernier ouvert) :
    /// `,` et `v` au clavier, l'écrou de la barre.
    @ObservationIgnored let openSettings: @MainActor (SettingsTab?) -> Void

    init(model: ReaderModel, store: DisplayModeStore? = nil,
         openSettings: @escaping @MainActor (SettingsTab?) -> Void = { _ in }) {
        self.model = model
        self.openSettings = openSettings
        self.store = store ?? DisplayModeStore(defaults: Self.processDefaults)
        // Avant la vue : son corps lit le placement (largeur du texte).
        placement = PanelPlacement(window: window, store: self.store, mode: { [unowned self] in self.mode })
        keys = PanelKeyRouter(window: window,
                              isEditing: { [unowned self] in self.editing },
                              action: { [unowned self] in self.action(for: $0) },
                              finishEditing: { [unowned self] in self.finishEditing() },
                              perform: { [unowned self] in self.perform($0) })
        announcements = PanelAnnouncements(window: window)
        let host = NSHostingView(rootView: PanelContent(controller: self))
        window.contentView = host
        host.setAccessibilityIdentifier("panneau-lecture")
        host.setAccessibilityLabel("Panneau de lecture")
        // Toute fermeture (⌘W, close(), performClose:) passe à l'arrière-plan.
        let guardian = PanelCloseGuard { [weak self] in self?.apply(.hide) }
        closeGuard = guardian
        window.delegate = guardian
        observeSettingsWindow()
    }

    /// Les préférences du processus, lues une fois : sous les tests, chaque
    /// appel à `processDefaults()` vide la suite.
    private static let processDefaults = DisplayModeStore.processDefaults()

    /// La largeur du texte du mode moyen : la copie cachée du mode petit la
    /// garde, pour que ↑ ↓ visent les mêmes lignes.
    var shadowTextWidth: CGFloat {
        PanelGeometry.panelFrame(contentHeight: 0, screen: placement.screenFrame).width
            - 2 * PanelGeometry.horizontalPadding
    }

    // MARK: - Modes

    /// Une transition (⌘1/2/3, Échap, ⌃⌥S, ⌃⌥Espace, fermeture), retenue
    /// puis appliquée.
    func apply(_ event: DisplayEvent) {
        let next = store.state.applying(event, playing: model.snapshot.playing)
        store.state = next
        present(next.current)
    }

    /// ⌃⌥S, « Afficher sillon » : le dernier mode visible.
    func show() {
        apply(.showLast)
    }

    /// Au lancement : le mode retenu reparaît, sans prendre le clavier à
    /// l'application au premier plan. Arrière-plan : rien à montrer.
    func restore() {
        let current = store.state.current
        guard current.isVisible else { return }
        present(current, takeKey: false)
    }

    /// ⏎ : le grand mode, depuis le moyen comme depuis la barre.
    func expand() {
        guard mode == .small || mode == .medium else { return }
        apply(.choose(.large))
    }

    /// Échap : du grand au moyen, sinon l'arrière-plan.
    func collapseOrHide() {
        apply(.escape)
    }

    private func present(_ target: DisplayMode, takeKey: Bool = true) {
        guard target.isVisible else { hideWindow(); return }
        // La barre du mode petit n'a pas de place pour écrire : la
        // modification s'applique avant.
        if editing, target == .small { finishEditing() }
        // Lu hors du fil principal : s'applique dès qu'il revient.
        reloadReading()
        let wasHidden = mode == .background || !window.isVisible
        if wasHidden || target != mode {
            mode = target
            placement.applySizeLimits(for: target)
            placement.place(placement.frame(for: target, contentHeight: contentHeight), animated: !wasHidden)
            updateIdentifier()
        }
        if takeKey { window.makeKeyAndOrderFront(nil) } else { window.orderFrontRegardless() }
        // La cadence de relecture suit ce qui est montré : le texte (à
        // chaque image en lecture), ou la barre (4 Hz en lecture, rien en
        // pause).
        model.presence = target == .small ? .bar : .full
        // L'annonceur ne voit rien en arrière-plan : on repart de l'état
        // courant pour ne rien annoncer de ce qui s'est passé entre-temps.
        model.refresh()
        announcements.reseed(with: model.snapshot)
        keys.install()
    }

    /// L'arrière-plan : la fenêtre se retire, la lecture continue. Le modèle
    /// ne relit plus l'état qu'une fois par seconde, et seulement s'il lit.
    private func hideWindow() {
        guard mode != .background else { return }
        // Une modification en cours n'est pas perdue : elle s'applique.
        if editing { finishEditing() }
        // Une recherche en cours ne survit pas : sinon le grand mode reviendrait
        // avec un champ vide et un compteur « n / m » périmé.
        if searchActive { searchActive = false; model.send(.searchCancel) }
        mode = .background
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

    /// L'action d'une touche, avec le pas d'avance rapide des réglages.
    func action(for press: KeyPress) -> ReaderAction? {
        KeyMap.action(for: press, seekStep: Int32(reading.seekStep))
    }

    /// La phrase à dire, ou rien : jamais en arrière-plan (fenêtre retirée).
    /// L'annonceur voit tous les instantanés visibles, VoiceOver ou non, pour
    /// que son état reste à jour.
    func announcementText(for s: Snapshot) -> String? {
        announcements.text(for: s, doc: model.document, background: mode == .background)
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

    func contentHeightChanged(_ height: CGFloat) {
        guard height != contentHeight else { return }
        contentHeight = height
        // Appelé pendant la mise à jour SwiftUI : redimensionner la fenêtre
        // au milieu de cette passe est annulé. On le fait au tour suivant,
        // une seule fois pour toutes les hauteurs reçues entre-temps.
        guard !resizePending else { return }
        resizePending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.resizePending = false
            guard self.mode == .medium else { return }
            self.placement.place(PanelGeometry.panelFrame(contentHeight: self.contentHeight,
                                                          screen: self.placement.screenFrame), animated: true)
        }
    }

    func perform(_ action: ReaderAction) {
        switch action {
        case .command(let command): model.send(command)
        case .line(let delta): moveLine(delta)
        case .expand: expand()
        case .display(let target): apply(.choose(target))
        case .collapseOrHide:
            if editing { finishEditing() }
            else if helpVisible { helpVisible = false }
            else if searchActive { searchActive = false; model.send(.searchCancel) }
            else { collapseOrHide() }
        case .copySentence:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(model.currentSentenceText, forType: .string)
            flash("Phrase copiée")
        case .openSearch:
            if mode != .large { apply(.choose(.large)) }
            searchActive = true
            openSearchRequests &+= 1
        case .toggleSidebar:
            if mode == .large { sidebarVisible.toggle() }
        case .toggleBookmarks:
            if mode != .large { apply(.choose(.large)) }
            sidebarVisible = true
            bookmarksVisible.toggle()
        case .showHelp:
            // La barre du mode petit n'a pas la place : l'aide s'ouvre au moyen.
            if mode == .small {
                apply(.choose(.medium))
                helpVisible = true
            } else {
                helpVisible.toggle()
            }
        case .openSettings:
            openSettings(nil)
        case .openVoiceSettings:
            openSettings(.voice)
        case .edit:
            beginEditing()
        }
    }

    // MARK: - Édition

    /// Double clic ou « e » : la lecture se met en pause et le texte devient
    /// modifiable. Le mode petit n'a pas la place : le panneau passe au moyen.
    func beginEditing() {
        guard !editing, model.snapshot.hasDocument else { return }
        resumeAfterEditing = model.snapshot.playing
        if resumeAfterEditing { model.send(.pause) }
        if searchActive { searchActive = false; model.send(.searchCancel) }
        helpVisible = false
        if mode == .small || mode == .background { apply(.choose(.medium)) }
        draftOrigin = model.editableText
        draft = draftOrigin
        editing = true
        // Le panneau moyen prend la place d'écrire.
        if mode == .medium { contentHeightChanged(.greatestFiniteMagnitude) }
    }

    /// Échap, ⌘↩ ou « Terminé » : le texte modifié remplace le document, la
    /// voix reprend à la même phrase. Rien de changé : la lecture reprend
    /// telle quelle.
    func finishEditing() {
        guard editing else { return }
        editing = false
        if draft != draftOrigin, !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            model.reopen(edited: draft, play: resumeAfterEditing)
            flash("Texte modifié")
        } else if resumeAfterEditing {
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
        let identifier: String
        let label: String
        switch mode {
        case .small:
            identifier = "lecteur-reduit"; label = "Lecteur réduit"
        case .large:
            identifier = "lecteur-agrandi"; label = "Lecteur agrandi"
        case .medium, .background:
            identifier = "panneau-lecture"; label = "Panneau de lecture"
        }
        window.contentView?.setAccessibilityIdentifier(identifier)
        window.contentView?.setAccessibilityLabel(label)
    }
}

/// Le délégué de la fenêtre : une fermeture demandée devient l'arrière-plan.
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

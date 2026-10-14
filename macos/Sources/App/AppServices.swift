import Observation
import SwiftUI

/// Ce que les scènes SwiftUI (barre des menus, réglages) reçoivent de l'app,
/// par l'environnement : le modèle et ses commandes, posés par `AppDelegate`
/// à la fin du lancement. Le corps des scènes est évalué avant : jusque-là
/// (et en mode mesure, où rien n'est créé), tout est nil et chaque action ne
/// fait rien. Observé : les vues se redessinent quand le modèle arrive.
@MainActor
@Observable
final class AppServices {
    var model: ReaderModel?
    var panel: PanelController?
    /// Lecture, pause, phrases et timeline venues d'ailleurs que du panneau.
    var transport: Transport?
    var notes: NotesWindowController?
    /// « Lire le presse-papiers » : le presse-papiers seul (depuis le menu).
    @ObservationIgnored var read: @MainActor () async -> Void = {}
}

/// Ouvre la fenêtre des réglages, sur un onglet ou le dernier ouvert.
struct OpenSettingsTabAction {
    let open: @MainActor (SettingsTab?) -> Void

    @MainActor func callAsFunction(_ tab: SettingsTab?) { open(tab) }
}

extension EnvironmentValues {
    /// Le panneau la pose (`PanelController.openSettings`) ; ailleurs, rien.
    @Entry var openSettingsTab = OpenSettingsTabAction { _ in }
}

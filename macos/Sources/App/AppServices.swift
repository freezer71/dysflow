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
    /// « Lire la sélection » : la sélection, sinon le presse-papiers.
    @ObservationIgnored var readSelection: @MainActor () async -> Void = {}
    /// « Ouvrir un fichier… » : le panneau d'ouverture.
    @ObservationIgnored var chooseFile: @MainActor () -> Void = {}
    /// La dictée (macOS 26) : nil sans elle.
    var dictation: DictationController?
}

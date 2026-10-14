import Observation

/// La sortie audio choisie (vide : celle du système) et les sorties
/// branchées. Le menu, les réglages et le panneau les lisent et les
/// changent par `ReaderModel` (`outputDevice`, `outputs`, `chooseOutput`).
@MainActor
@Observable
final class AudioOutputs {
    private(set) var device = ""
    private(set) var available = AudioOutputsView(devices: [], system: nil)
    /// Lit les sorties branchées auprès du moteur (environ 2 ms, mais sous
    /// le verrou du lecteur) : toujours hors du fil principal.
    @ObservationIgnored private let list: @Sendable () -> AudioOutputsView
    /// Des relectures qui se chevauchent : seule la dernière demandée compte.
    @ObservationIgnored private var generation = 0

    init(list: @escaping @Sendable () -> AudioOutputsView) {
        self.list = list
    }

    /// Relit les sorties branchées, hors du fil principal : à chaque sortie
    /// branchée ou retirée (`AudioDeviceWatcher`), jamais à chaque image.
    func refresh() {
        Task { await reload() }
    }

    /// Relit les sorties branchées et attend la réponse.
    func reload() async {
        generation &+= 1
        let asked = generation
        let list = list
        let next = await Task.detached { list() }.value
        guard asked == generation, next != available else { return }
        available = next
    }

    /// La sortie retenue, ou choisie : le menu la coche aussitôt.
    func select(_ name: String) {
        device = name
    }
}

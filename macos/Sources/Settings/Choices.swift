/// Les options d'un menu, plus la valeur en place si elle n'y est pas.
///
/// Une valeur écrite à la main dans `config.toml` (« lire », « ja » sur
/// MagpieTTS, « cuda ») s'affiche telle quelle : le menu ne la remplace pas
/// par sa première option, et rien n'est réécrit tant qu'on n'y touche pas.
enum Choices {
    static func including<T: Hashable>(_ options: [(title: String, value: T)], current: T,
                                      title: (T) -> String) -> [(title: String, value: T)] {
        options.contains { $0.value == current } ? options : options + [(title: title(current), value: current)]
    }
}

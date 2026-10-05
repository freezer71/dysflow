import Foundation

/// Mesures des seuils de la spec, sur le vrai moteur. Lancé par
/// `scripts/measure-macos.sh` (`Sillon --measure`) ; écrit `MESURE nom valeur
/// unité` sur la sortie standard, puis l'app quitte.
///
/// Deux réserves sur ce que les chiffres veulent dire :
/// - « premier son » = `Snapshot.audible` (lecture en cours ET position de
///   lecture connue) : la latence de sortie du périphérique audio n'y est pas
///   comprise, elle s'ajoute au ressenti.
/// - la mémoire est l'empreinte physique (`phys_footprint`), plus fidèle que le
///   RSS de `ps` utilisé à la tâche 1 : les deux ne sont pas comparables tel quel.
@MainActor
enum Measure {
    static let text = """
    Les monades permettent d'enchaîner des calculs qui portent un contexte. \
    On les retrouve partout : options, listes, promesses. \
    Une seconde phrase pour avoir de quoi sauter. Et une troisième.
    """

    /// Échec d'une mesure : la ligne `MESURE erreur …` est déjà écrite.
    struct Echec: Error {}

    static func run(reader: SillonReader) async {
        do {
            try await mesurer(reader: reader)
        } catch {
            // Rien à ajouter : l'erreur a été imprimée au point d'échec.
        }
    }

    private static func mesurer(reader: SillonReader) async throws {
        // À froid : voix au repos, puis le geste du raccourci.
        for i in 1...5 {
            reader.restVoice()
            try? await Task.sleep(for: .seconds(1))
            let t = ContinuousClock.now
            reader.prepareVoice()
            try await ouvrirEtLire(reader, source: "mesure-\(i)")
            try await attendreSon(reader)
            emit("froid_premier_son", t.duration(to: .now))
        }
        // Voix chargée.
        for i in 1...5 {
            reader.command(command: .pause)
            let t = ContinuousClock.now
            try await ouvrirEtLire(reader, source: "chaud-\(i)")
            try await attendreSon(reader)
            emit("chaud_premier_son", t.duration(to: .now))
        }
        // Sauts : phrase, puis mot.
        for _ in 1...5 {
            try await attendreSon(reader)
            let cible = reader.snapshot().sentence + 1
            let t = ContinuousClock.now
            reader.command(command: .sentences(delta: 1))
            try await attendre(reader) { $0.sentence == cible && $0.audible }
            emit("saut_phrase", t.duration(to: .now))
            reader.command(command: .sentences(delta: -1))
        }
        for _ in 1...5 {
            try await attendreSon(reader)
            let t = ContinuousClock.now
            reader.command(command: .words(delta: 3))
            try? await Task.sleep(for: .milliseconds(20))
            try await attendreSon(reader)
            emit("saut_mot", t.duration(to: .now))
        }
        // Mémoire au repos, sur dix cycles.
        for cycle in 1...10 {
            reader.prepareVoice()
            try await ouvrirEtLire(reader, source: "cycle-\(cycle)")
            try await attendreSon(reader)
            reader.restVoice()
            try? await Task.sleep(for: .seconds(1))
            print("MESURE memoire_repos_\(cycle) \(residentMB()) Mo")
            fflush(stdout)
        }
    }

    /// Ouvre le texte puis lance la lecture. La config réelle lit
    /// automatiquement (autoplay) : on attend que l'ouverture soit prise en
    /// compte (la révision change) et on ne bascule que si ça ne joue pas, sinon
    /// on mettrait en pause.
    private static func ouvrirEtLire(_ reader: SillonReader, source: String) async throws {
        let avant = reader.snapshot().revision
        reader.openText(markdown: text, source: source)
        try await attendre(reader) { $0.revision != avant }
        if !reader.snapshot().playing {
            reader.command(command: .togglePlay)
        }
    }

    private static func attendreSon(_ reader: SillonReader) async throws {
        try await attendre(reader) { $0.audible }
    }

    /// Attend la condition (10 s au plus, sondage toutes les 2 ms). Au délai
    /// dépassé : voix indisponible si elle n'est pas prête, sinon simple délai.
    private static func attendre(_ reader: SillonReader, _ condition: (Snapshot) -> Bool) async throws {
        let echeance = ContinuousClock.now + .seconds(10)
        while !condition(reader.snapshot()) {
            if ContinuousClock.now >= echeance {
                switch reader.snapshot().voice {
                case .resting, .fallback:
                    print("MESURE erreur voix-indisponible")
                default:
                    print("MESURE erreur delai-depasse")
                }
                fflush(stdout)
                throw Echec()
            }
            try? await Task.sleep(for: .milliseconds(2))
        }
    }

    private static func emit(_ name: String, _ d: Duration) {
        let ms = Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15
        print(String(format: "MESURE %@ %.0f ms", name, ms))
        fflush(stdout)
    }

    private static func residentMB() -> Int {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? Int(info.phys_footprint / 1_048_576) : -1
    }
}

// xcstrings: garder tout le fichier (sortie de mesure et textes lus à voix haute, en français)
#if DEBUG || SILLAGE_MESURE
import Foundation
import KokoroMLX

/// Mesures des seuils de la spec, sur le vrai moteur. Lancé par
/// `scripts/measure-macos.sh` (`Sillage --measure`) ; écrit `MESURE nom valeur
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
    Une seconde phrase pour avoir de quoi sauter. Et une troisième. \
    Une quatrième phrase, pour que la lecture automatique ne finisse pas. \
    Une cinquième suit, avec assez de mots pour plusieurs sauts successifs. \
    Une sixième rallonge encore le texte, sans rien dire de plus. \
    Une septième, puis une huitième, afin de ne jamais atteindre la fin. \
    Une neuvième phrase, longue à dessein, pour couvrir toute la série. \
    Enfin une dixième, qui clôt ce texte de mesure.
    """

    /// Échec d'une mesure : la ligne `MESURE erreur …` est déjà écrite.
    struct Echec: Error {}

    static func run(reader: SillageReader) async {
        do {
            try await mesurer(reader: reader)
        } catch {
            // Rien à ajouter : l'erreur a été imprimée au point d'échec.
        }
    }

    private static func mesurer(reader: SillageReader) async throws {
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
            let depart = reader.snapshot().sentence
            let t = ContinuousClock.now
            reader.command(command: .sentences(delta: 1))
            try await attendre(reader) { $0.sentence == depart + 1 && $0.audible }
            emit("saut_phrase", t.duration(to: .now))
            // Retour, confirmé : sans cela l'itération suivante lirait un
            // instantané périmé et viserait une phrase que le lecteur dépasse.
            reader.command(command: .sentences(delta: -1))
            try await attendre(reader) { $0.sentence == depart && $0.audible }
        }
        for _ in 1...5 {
            try await attendreSon(reader)
            let avant = reader.snapshot()
            let t = ContinuousClock.now
            reader.command(command: .words(delta: 3))
            // Le chrono ne s'arrête que sur l'effet du saut lui-même : `word`
            // est un index global, le saut fait +3 ; l'avance naturelle de la
            // lecture ne fait que +1 en quelques ms et ne doit pas compter.
            // Sans mot connu, un changement de phrase tient lieu de preuve.
            try await attendre(reader) { s in
                guard s.audible else { return false }
                if let mot = s.word, let avantMot = avant.word {
                    return mot >= avantMot + 3
                }
                return s.sentence != avant.sentence
            }
            emit("saut_mot", t.duration(to: .now))
        }
        // Mémoire au repos, sur dix cycles.
        for cycle in 1...10 {
            reader.prepareVoice()
            try await ouvrirEtLire(reader, source: "cycle-\(cycle)")
            try await attendreSon(reader)
            reader.restVoice()
            // Échantillon seulement une fois la voix réellement relâchée.
            try await attendre(reader) { $0.voice == .resting }
            print("MESURE memoire_repos_\(cycle) \(residentMB()) Mo")
            fflush(stdout)
        }
    }

    /// Ouvre le texte puis lance la lecture. La config réelle lit
    /// automatiquement (autoplay) : on attend que l'ouverture soit prise en
    /// compte (la révision change) et on ne bascule que si ça ne joue pas, sinon
    /// on mettrait en pause.
    private static func ouvrirEtLire(_ reader: SillageReader, source: String) async throws {
        let avant = reader.snapshot().revision
        reader.openText(markdown: text, source: source)
        try await attendre(reader) { $0.revision != avant }
        if !reader.snapshot().playing {
            reader.command(command: .togglePlay)
        }
    }

    private static func attendreSon(_ reader: SillageReader) async throws {
        try await attendre(reader) { $0.audible }
    }

    /// Attend la condition (10 s au plus, sondage toutes les 2 ms). Au délai
    /// dépassé : voix indisponible si elle n'est pas prête, sinon simple délai.
    private static func attendre(_ reader: SillageReader, _ condition: (Snapshot) -> Bool) async throws {
        let echeance = ContinuousClock.now + .seconds(10)
        while !condition(reader.snapshot()) {
            if ContinuousClock.now >= echeance {
                // L'état au délai dépassé, pour savoir où la lecture s'est
                // arrêtée ; jamais le texte du document.
                let s = reader.snapshot()
                print("MESURE etat document=\(s.hasDocument) lecture=\(s.playing) audible=\(s.audible) fini=\(s.finished) phrase=\(s.sentence) revision=\(s.revision) voix=\(s.voice) statut=\(s.status) sautees=\(s.skipped)")
                switch s.voice {
                case .resting, .fallback, .unavailable:
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

    nonisolated static func residentMB() -> Int {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let kr = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return kr == KERN_SUCCESS ? Int(info.phys_footprint / 1_048_576) : -1
    }

    /// Un texte long : de quoi lire sept minutes sans atteindre la fin.
    static let longText = Array(repeating: """
    Le 14 juillet 1789, Camille Desmoulins harangua la foule devant le Palais-Royal, \
    et Paris bascula en quelques heures. Le rapport de la commission, long de 37 pages, \
    est arrivé un mardi ; personne n'a lu plus loin que le résumé. Pour corriger le bug, \
    lancez les tests, puis vérifiez que la configuration renvoie une erreur au lieu de planter. \
    Marguerite habitait depuis 41 ans la même maison étroite de la rue des Tanneurs, \
    et pendant tout ce temps, elle n'avait jamais fermé la porte du jardin à clé.
    """, count: 14).joined(separator: "\n\n")

    /// Le filtre de charge de la spec. Les phases s'annoncent sur la sortie
    /// standard (`PHASE nom`), avec les compteurs du moteur (`COMPTEUR …`) ;
    /// `scripts/mesure-charge.py` échantillonne CPU, GPU et mémoire.
    ///
    /// Aucun fichier de l'utilisateur n'est touché : `openText` ouvre un texte
    /// sans identité stable, dont la position n'est jamais retenue
    /// (`positions.json` reste intact). Seule la vitesse passe par la
    /// configuration ; elle est rendue ici dans tous les cas, échec compris.
    static func charge(reader: SillageReader, compute: KokoroMLXCompute?) async {
        let vitesse = reader.snapshot().speed
        do {
            try await mesurerCharge(reader: reader, compute: compute)
        } catch {
            // L'erreur a été écrite au point d'échec (`MESURE erreur …`).
        }
        reader.command(command: .pause)
        reader.command(command: .setSpeed(speed: vitesse))
        phase("fin")
    }

    private static func phase(_ name: String) {
        print("PHASE \(name)")
        fflush(stdout)
    }

    private static func counters(_ reader: SillageReader, _ compute: KokoroMLXCompute?) {
        let m = reader.measures()
        print(String(format: "COMPTEUR busy_s=%.3f stalls=%u mlx_calls=%ld forward_s=%.3f", m.busySeconds, m.stalls, compute?.calls ?? -1, compute?.forwardSeconds ?? -1))
        fflush(stdout)
    }

    private static func dormir(_ seconds: Int) async {
        try? await Task.sleep(for: .seconds(seconds))
    }

    private static func mesurerCharge(reader: SillageReader, compute: KokoroMLXCompute?) async throws {
        // Repos : l'app sans voix, puis le GPU de la machine au repos.
        reader.restVoice()
        phase("baseline")
        await dormir(20)
        print("MESURE empreinte_sans_voix \(residentMB()) Mo")
        fflush(stdout)

        // Chargement, ouverture et sauts : hors de la ligne de base du GPU.
        phase("charge_voix")
        reader.prepareVoice()
        try await attendreVoixKokoro(reader)
        reader.command(command: .setSpeed(speed: 1))
        let avant = reader.snapshot().revision
        reader.openText(markdown: longText, source: "charge")
        try await attendre(reader) { $0.revision != avant }
        if !reader.snapshot().playing { reader.command(command: .togglePlay) }
        try await attendreSon(reader)
        // Le son ne prouve pas MLX (la voix de repli en fait aussi) : au moins
        // un passage MLX doit avoir eu lieu.
        guard (compute?.calls ?? 0) > 0 else { throw refus("aucun-appel-mlx") }

        // Premier son à chaud après un saut loin : la phrase visée n'est pas
        // prête, elle part en tête et suite.
        // Le texte compte 56 phrases : les cibles 10, 20… 50 restent dedans.
        for i in 1...5 {
            let cible = UInt32(10 * i)
            let t = ContinuousClock.now
            reader.command(command: .seekTo(sentence: cible))
            try await attendre(reader) { $0.sentence == cible && $0.audible }
            emit("saut_loin_premier_son", t.duration(to: .now))
        }
        reader.command(command: .seekTo(sentence: 0))
        try await attendreSon(reader)

        phase("advance")
        await dormir(10)

        phase("read_1x")
        counters(reader, compute)
        await dormir(120)
        counters(reader, compute)
        try verifierEnCours(reader, "read_1x")

        reader.command(command: .setSpeed(speed: 2))
        phase("read_2x")
        counters(reader, compute)
        await dormir(120)
        counters(reader, compute)
        try verifierEnCours(reader, "read_2x")

        // Coût de la copie des échantillons à travers le pont UniFFI.
        mesurerPont()

        // Pause : la fenêtre de synthèse en avance se remplit, puis plus rien.
        reader.command(command: .pause)
        phase("pause_settle")
        await dormir(5)
        phase("pause")
        counters(reader, compute)
        await dormir(20)
        counters(reader, compute)

        // Repos : modèle, cache MLX et espeak-ng libérés.
        phase("rest")
        reader.restVoice()
        try await attendre(reader) { $0.voice == .resting }
        await dormir(5)
        print("MESURE empreinte_repos \(residentMB()) Mo")
        fflush(stdout)
    }

    /// Nom du moteur Kokoro sur MLX dans l'instantané (`KokoroEngine::name`,
    /// étiquette « MLX » du calcul de l'app).
    static let moteurKokoroMLX = "Kokoro · MLX"

    /// Prête ne suffit pas : c'est Kokoro sur MLX qui doit parler.
    static func voixKokoroMLXPrete(_ s: Snapshot) -> Bool {
        s.voice == .ready && s.engine == moteurKokoroMLX
    }

    /// Attend que la voix demandée soit prête (60 s au plus). Une voix de repli,
    /// un modèle absent, ou une voix « prête » d'un autre moteur (un moteur qui
    /// ne sait pas lire le `.safetensors`, muet) n'est pas Kokoro MLX : la mesure s'arrête,
    /// elle ne mesurerait pas ce que la spec demande.
    private static func attendreVoixKokoro(_ reader: SillageReader) async throws {
        let debut = ContinuousClock.now
        let echeance = debut + .seconds(60)
        while true {
            let s = reader.snapshot()
            let etat = s.voice
            switch etat {
            case .resting where ContinuousClock.now < debut + .seconds(5):
                break  // la demande n'a pas encore été prise en compte
            case .ready:
                guard voixKokoroMLXPrete(s) else {
                    throw refus("voix-non-kokoro-mlx-moteur-\(s.engine.isEmpty ? "inconnu" : s.engine)")
                }
                print("MESURE voix ready moteur=\(s.engine)")
                fflush(stdout)
                return
            case .fallback, .resting, .unavailable:
                throw refus("voix-non-kokoro-mlx-\(String(describing: etat).prefix(8))")
            case .loading:
                break
            }
            if ContinuousClock.now >= echeance { throw refus("voix-jamais-prete") }
            try? await Task.sleep(for: .milliseconds(50))
        }
    }

    /// La lecture doit encore être en cours : un texte fini ou une pause
    /// feraient passer « aucun appel » pour un succès.
    private static func verifierEnCours(_ reader: SillageReader, _ phase: String) throws {
        let s = reader.snapshot()
        print("MESURE phrase_apres_\(phase) \(s.sentence) phrase")
        fflush(stdout)
        if s.finished || !s.playing { throw refus("lecture-arretee-\(phase)") }
    }

    private static func refus(_ raison: String) -> Echec {
        print("MESURE erreur \(raison)")
        fflush(stdout)
        return Echec()
    }

    /// La copie d'une phrase à travers le pont UniFFI (`Vec<f32>`) : écriture
    /// puis relecture, toutes deux faites ici par le code Swift généré, pour
    /// 2, 5 et 10 s de son à 24 kHz. C'est une BORNE HAUTE du coût réel (en
    /// service, Swift écrit et Rust relit une seule fois). Trois passes
    /// d'échauffement, puis la médiane de 25 répétitions. Le pilote la
    /// rapporte à la durée moyenne d'un passage MLX (`pont_pour_cent_synthese`).
    /// Les échantillons sont du silence : rien du texte lu n'y passe.
    private static func mesurerPont() {
        func unePasse(_ sortie: KokoroOutput) -> (ok: Bool, ms: Double) {
            let t = ContinuousClock.now
            let rendu = try? FfiConverterTypeKokoroOutput_lift(FfiConverterTypeKokoroOutput_lower(sortie))
            let d = t.duration(to: .now)
            return (rendu != nil, Double(d.components.seconds) * 1000 + Double(d.components.attoseconds) / 1e15)
        }
        for secondes in [2, 5, 10] {
            let sortie = KokoroOutput(samples: [Float](repeating: 0, count: 24_000 * secondes),
                                      durations: [UInt32](repeating: 2, count: 100))
            for _ in 0..<3 { _ = unePasse(sortie) }
            var durees: [Double] = []
            for _ in 0..<25 {
                let r = unePasse(sortie)
                if !r.ok { print("MESURE erreur pont-illisible"); break }
                durees.append(r.ms)
            }
            guard !durees.isEmpty else { continue }
            print(String(format: "MESURE pont_copie_%lds %.2f ms", secondes, durees.sorted()[durees.count / 2]))
        }
        fflush(stdout)
    }

    // MARK: - Cadence d'un modèle à texte (lot 3)

    /// Huit phrases françaises, quatre anglaises : longueurs de lecture
    /// ordinaires (formes parlées, sans chiffres).
    nonisolated static let rtfPhrases: [(language: String, text: String)] = [
        ("fr", "Les monades permettent d'enchaîner des calculs qui portent un contexte."),
        ("fr", "On les retrouve partout : options, listes, promesses."),
        ("fr", "Ce matin, le marché était plein, et les étals débordaient de fruits."),
        ("fr", "Il nous fallut deux heures pour atteindre le sommet de ce pic."),
        ("fr", "La pluie tombait sans bruit sur les toits de la vieille ville."),
        ("fr", "Elle referma le livre, puis regarda longtemps par la fenêtre."),
        ("fr", "Une phrase plus courte."),
        ("fr", "Enfin, une dernière phrase, un peu plus longue que les autres, pour finir la série."),
        ("en", "The quick brown fox jumps over the lazy dog."),
        ("en", "Monads let you chain computations that carry a context."),
        ("en", "It was raining softly over the roofs of the old town."),
        ("en", "She closed the book and looked out of the window for a long time."),
    ]

    /// La plus grande vitesse tenue avec 20 % de marge : à la vitesse v, la
    /// voix est occupée `rtf × v` du temps ; au-delà de 80 %, le moindre
    /// à-coup fait attendre la lecture. `nil` : même 1× ne tient pas.
    nonisolated static func fluentSpeed(rtf: Double) -> Double? {
        [3, 2.5, 2, 1.5, 1.25, 1].first { rtf * $0 <= 0.8 }
    }

    /// La phrase de la fiche (`pace`), d'après la mesure.
    nonisolated static func paceSentence(rtf: Double) -> String {
        let busy = Int((rtf * 100).rounded())
        guard let speed = fluentSpeed(rtf: rtf) else {
            return "Lourd : lecture hachée même à 1× (occupé \(busy) % du temps sur ce Mac)."
        }
        let x = speed.formatted(.number.precision(.fractionLength(0...2)).locale(Locale(identifier: "fr_FR")))
        if speed >= 3 {
            let times = Int((1 / rtf).rounded(.down))
            let words = [2: "Deux", 3: "Trois", 4: "Quatre", 5: "Cinq", 6: "Six", 7: "Sept", 8: "Huit", 9: "Neuf"]
            let lead = words[times].map { "\($0) fois" } ?? "\(times) fois"
            return "\(lead) plus vite que la parole sur ce Mac : lecture fluide jusqu'à \(x)×."
        }
        return "Lourd : lecture hachée au-delà de \(x)× (occupé \(busy) % du temps à 1× sur ce Mac)."
    }

    /// Charge le modèle `id` (installé), le prépare, puis synthétise les
    /// phrases une à une ; écrit le RTF soutenu par langue (calcul / durée
    /// du son), la vitesse tenue, la phrase de la fiche et l'empreinte après
    /// la série. Le calcul tourne hors du fil principal (il bloque sous le
    /// verrou MLX) ; aucune fenêtre ne s'ouvre, l'appelant quitte l'app.
    static func rtf(reader: SillageReader, compute: TextSpeechMLXCompute?, id: String) async {
        defer { fflush(stdout) }
        guard !id.isEmpty else {
            print("MESURE erreur id manquant"); return
        }
        guard let compute else {
            print("MESURE erreur \(id) pas de calcul MLX (lecteur muet)"); return
        }
        guard let dir = reader.modelDirectory(id: id) else {
            print("MESURE erreur \(id) non installé"); return
        }
        await empreinteSansVoix()
        let result = await Task.detached { try serieRTF(compute: compute, dir: dir, id: id) }.result
        switch result {
        case .failure(let error):
            print("MESURE erreur \(id) \(TextSpeechMLXCompute.message(of: error))")
        case .success(let serie):
            func ratio(_ language: String) -> Double {
                serie.calcul[language, default: 0] / serie.son[language, default: 0]
            }
            for language in ["fr", "en"] {
                print(String(format: "MESURE rtf %@ %@ %.3f", id, language, ratio(language)))
            }
            let fr = ratio("fr")
            print("MESURE cadence_max \(id) \(fluentSpeed(rtf: fr).map { String($0) } ?? "aucune")")
            print("MESURE fiche \(id) \(paceSentence(rtf: fr))")
            print("MESURE empreinte \(id) \(serie.empreinte) Mo")
        }
        await empreinteRepos(id)
    }

    /// L'app sans voix chargée : la ligne de base des mesures au repos.
    static func empreinteSansVoix() async {
        try? await Task.sleep(for: .seconds(2))
        print("MESURE pid \(getpid())")
        print("MESURE empreinte_sans_voix \(residentMB()) Mo \(compteursMLX())")
        fflush(stdout)
    }

    /// Le modèle libéré (`release` rendu) : l'empreinte au repos, puis toutes
    /// les 0,5 s pendant 20 s (le pilote Metal rend la mémoire en ~2 s ; de
    /// quoi lancer `vmmap --summary <pid>`).
    static func empreinteRepos(_ id: String) async {
        print("MESURE empreinte_repos \(id) \(residentMB()) Mo \(compteursMLX())")
        fflush(stdout)
        let debut = ContinuousClock.now
        for _ in 0..<40 {
            try? await Task.sleep(for: .milliseconds(500))
            let t = debut.duration(to: .now)
            print(String(format: "MESURE repos_t %.1f s %d Mo",
                         Double(t.components.seconds) + Double(t.components.attoseconds) / 1e18, residentMB()))
            fflush(stdout)
        }
        print("MESURE empreinte_repos_20s \(id) \(residentMB()) Mo \(compteursMLX())")
        fflush(stdout)
    }

    /// Ce que MLX tient : tableaux vivants et cache, en Mo.
    nonisolated static func compteursMLX() -> String {
        let c = DictationMLX.counters()
        return "mlx_actif=\(c.active / 1_048_576) mlx_cache=\(c.cache / 1_048_576)"
    }

    /// Le chemin du moteur Rust, sans son : la voix de la configuration
    /// préparée (chargement et préparation sur les fils du moteur), puis
    /// mise au repos ; l'empreinte prête, puis au repos. La configuration
    /// est celle de `HOME` : lancer avec un `HOME` de mesure (copie de
    /// `config.toml`, `volume = 0.0`, lien vers `models/`) pour ne rien
    /// entendre ni toucher aux réglages de l'utilisateur.
    static func reposLecteur(reader: SillageReader) async {
        defer { fflush(stdout) }
        reader.restVoice()
        await empreinteSansVoix()
        reader.prepareVoice()
        let echeance = ContinuousClock.now + .seconds(120)
        while reader.snapshot().voice != .ready {
            if ContinuousClock.now >= echeance {
                print("MESURE erreur voix \(reader.snapshot().voice) moteur=\(reader.snapshot().engine)"); return
            }
            try? await Task.sleep(for: .milliseconds(100))
        }
        let moteur = reader.snapshot().engine
        print("MESURE empreinte lecteur_pret \(residentMB()) Mo \(compteursMLX()) moteur=\(moteur)")
        // Une vraie lecture (le volume vient de la configuration : 0 pour
        // une mesure muette), 20 s, puis la pause.
        let avant = reader.snapshot().revision
        reader.openText(markdown: longText, source: "repos")
        while reader.snapshot().revision == avant { try? await Task.sleep(for: .milliseconds(20)) }
        if !reader.snapshot().playing { reader.command(command: .togglePlay) }
        try? await Task.sleep(for: .seconds(20))
        print("MESURE empreinte lecteur \(residentMB()) Mo \(compteursMLX()) phrase=\(reader.snapshot().sentence)")
        reader.command(command: .pause)
        try? await Task.sleep(for: .seconds(2))
        reader.restVoice()
        while reader.snapshot().voice != .resting { try? await Task.sleep(for: .milliseconds(50)) }
        await empreinteRepos("lecteur")
    }

    /// L'aperçu de la bibliothèque seul (aucune voix de lecture chargée) :
    /// le modèle `id` chargé pour l'aperçu, puis la bibliothèque laissée
    /// ouverte ; l'empreinte toutes les 5 s pendant `secondes`. Le délai de
    /// garde (`keep_voice_minutes`) doit le mettre au repos. Même `HOME` de
    /// mesure que `reposLecteur` (l'aperçu sonne au volume configuré).
    static func reposApercu(reader: SillageReader, id: String, secondes: Int) async {
        defer { fflush(stdout) }
        reader.restVoice()
        await empreinteSansVoix()
        if ProcessInfo.processInfo.environment["SILLAGE_MESURE_PREPARER"] == "1" {
            reader.prepareVoice()
            while reader.snapshot().voice != .ready { try? await Task.sleep(for: .milliseconds(100)) }
            print("MESURE voix_lecture \(reader.snapshot().engine) \(residentMB()) Mo \(compteursMLX())")
        }
        do {
            try reader.previewVoiceWith(modelId: id, voice: "", text: "Bonjour.", speed: 1)
        } catch {
            print("MESURE erreur apercu \(error)"); return
        }
        let debut = ContinuousClock.now
        while debut.duration(to: .now) < .seconds(secondes) {
            try? await Task.sleep(for: .seconds(5))
            let t = debut.duration(to: .now).components.seconds
            print("MESURE apercu_t \(t) s \(residentMB()) Mo \(compteursMLX())")
            fflush(stdout)
        }
        print("MESURE empreinte_repos apercu \(residentMB()) Mo \(compteursMLX())")
    }

    /// Kokoro : chargé, préparé (une synthèse), puis libéré ; empreinte
    /// chargé et au repos.
    static func reposKokoro(compute: KokoroMLXCompute?) async {
        defer { fflush(stdout) }
        guard let compute else { print("MESURE erreur kokoro pas de calcul MLX"); return }
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("sillage/models/kokoro").path
        await empreinteSansVoix()
        let result = await Task.detached { () throws -> Int in
            try compute.load(modelDir: dir)
            defer { compute.release() }
            try compute.warmUp()
            return residentMB()
        }.result
        switch result {
        case .failure(let error): print("MESURE erreur kokoro \(error)")
        case .success(let mo): print("MESURE empreinte kokoro \(mo) Mo")
        }
        await empreinteRepos("kokoro")
    }

    /// Une série mesurée : secondes de calcul et secondes de son par langue,
    /// empreinte (Mo) relevée le modèle encore chargé.
    struct SerieRTF: Sendable {
        var calcul: [String: Double] = [:]
        var son: [String: Double] = [:]
        var empreinte = -1
    }

    /// La série elle-même. Une synthèse sans un seul échantillon est une
    /// erreur : un RTF proche de zéro passerait pour « fluide à 3× ».
    private nonisolated static func serieRTF(compute: TextSpeechMLXCompute, dir: String,
                                             id: String) throws -> SerieRTF {
        let runtime = id.hasPrefix("magpie") ? TextSpeechMLXCompute.magpieRuntime : TextSpeechMLXCompute.qwenRuntime
        let voice = id.hasPrefix("magpie") ? "sofia" : (id.contains("customvoice") ? "vivian" : "siwis")
        try compute.load(modelDir: dir, runtime: runtime)
        defer { compute.release(modelDir: dir) }
        try compute.warmUp(modelDir: dir)
        var serie = SerieRTF()
        for (language, text) in rtfPhrases {
            let start = ContinuousClock.now
            let out = try compute.synthesize(modelDir: dir, text: text, language: language, voice: voice,
                                             isCancelled: { false })
            let elapsed = start.duration(to: .now)
            guard !out.samples.isEmpty, out.sampleRate > 0 else {
                throw SillageError.Failed(message: "synthèse vide (\(language)) : « \(text) »")
            }
            serie.calcul[language, default: 0] += Double(elapsed.components.seconds)
                + Double(elapsed.components.attoseconds) / 1e18
            serie.son[language, default: 0] += Double(out.samples.count) / Double(out.sampleRate)
        }
        // Avant la libération (le `defer`) : l'empreinte compte le modèle.
        serie.empreinte = residentMB()
        return serie
    }
}
#endif

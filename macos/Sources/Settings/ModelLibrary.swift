import Foundation

/// La bibliothèque des modèles de dictée, sans interface : ses filtres,
/// leur ordre et ce que VoiceOver dit d'une ligne. Les lignes sont celles de
/// la table de la dictée (`DictationSection.rows`), l'utilisée marquée
/// `active` d'après le choix des réglages.
enum ModelLibrary {
    /// Les filtres de la feuille ; vide : tout.
    struct Filter: Equatable {
        /// Le nom de l'éditeur (`ModelMaker.name`).
        var maker = ""
        var language = ""
        var state = ""
        var search = ""
    }

    static let all = "Tous"

    /// Les valeurs du filtre « Langue ».
    static let french = "fr"
    static let englishOnly = "en-seul"

    /// Les valeurs du filtre « État ».
    static let installed = "installes"
    static let missing = "a-telecharger"

    /// Le modèle utilisé d'abord, puis ceux qui sont là (la dictée de macOS
    /// en est), puis par éditeur et par nom.
    static func ordered(_ rows: [VoiceModel]) -> [VoiceModel] {
        rows.sorted { a, b in
            if a.active != b.active { return a.active }
            if isThere(a) != isThere(b) { return isThere(a) }
            let ma = ModelTable.maker(a).name, mb = ModelTable.maker(b).name
            if ma != mb { return ma.localizedStandardCompare(mb) == .orderedAscending }
            return a.name.localizedStandardCompare(b.name) == .orderedAscending
        }
    }

    /// Installé, ou la dictée de macOS : rien à télécharger, on peut l'utiliser.
    static func isThere(_ row: VoiceModel) -> Bool {
        row.installed || row.system
    }

    /// Les lignes qui passent les filtres, dans l'ordre de la bibliothèque.
    /// La recherche porte sur le nom, sans casse ni accents.
    static func filtered(_ rows: [VoiceModel], _ filter: Filter) -> [VoiceModel] {
        let search = filter.search.trimmingCharacters(in: .whitespacesAndNewlines)
        return ordered(rows).filter { row in
            (filter.maker.isEmpty || ModelTable.maker(row).name == filter.maker)
                && matches(row, language: filter.language)
                && matches(row, state: filter.state)
                && (search.isEmpty
                    || row.name.range(of: search, options: [.caseInsensitive, .diacriticInsensitive]) != nil)
        }
    }

    /// « Français » : le modèle écrit le français ; « Anglais seulement » :
    /// il n'écrit que l'anglais.
    static func matches(_ row: VoiceModel, language: String) -> Bool {
        switch language {
        case french: row.languages.contains("fr")
        case englishOnly: row.languages == ["en"]
        default: true
        }
    }

    /// « Installés » : là sur ce Mac (la dictée de macOS comprise) ; « À
    /// télécharger » : le reste, en cours ou échoué compris.
    static func matches(_ row: VoiceModel, state: String) -> Bool {
        switch state {
        case installed: isThere(row)
        case missing: !isThere(row)
        default: true
        }
    }

    /// « Tous », puis chaque éditeur présent, par ordre alphabétique.
    static func makerOptions(_ rows: [VoiceModel]) -> [(title: String, value: String)] {
        let makers = Set(rows.map { ModelTable.maker($0).name })
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
        return [(title: all, value: "")] + makers.map { (title: $0, value: $0) }
    }

    static let languageOptions: [(title: String, value: String)] =
        [(title: "Toutes", value: ""), (title: "Français", value: french),
         (title: "Anglais seulement", value: englishOnly)]

    static let stateOptions: [(title: String, value: String)] =
        [(title: all, value: ""), (title: "Installés", value: installed), (title: "À télécharger", value: missing)]

    /// La ligne sous le nom : langues et précision.
    static func details(_ row: VoiceModel) -> String {
        [row.languagesLabel, row.precision].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// La ligne choisie, si les filtres la montrent : une ligne cachée n'est
    /// pas utilisée.
    static func selected(_ visible: [VoiceModel], selection: String?) -> VoiceModel? {
        visible.first { $0.id == selection }
    }

    /// « Utiliser » : le modèle choisi est là. L'utilisé aussi (la feuille
    /// se ferme sans rien changer).
    static func canUse(_ row: VoiceModel?) -> Bool {
        row.map(isThere) ?? false
    }

    /// Ce que VoiceOver dit d'une ligne : nom, éditeur, langues, état et
    /// taille, puis s'il est utilisé.
    static func accessibilityLabel(_ row: VoiceModel, _ state: ModelRowState) -> String {
        var parts = [row.name, ModelTable.maker(row).name, row.languagesLabel].filter { !$0.isEmpty }
        switch state {
        case .system: parts.append("sur ce Mac")
        case .notInstalled: parts.append("non installé, \(ModelTable.size(row)) à télécharger")
        case .downloading(let fraction): parts.append("téléchargement \(Int(fraction * 100)) %")
        case .failed(let message): parts.append("échec du téléchargement : \(message)")
        case .installed: parts.append("installé, \(ModelTable.size(row))")
        }
        if state != .installed, ModelTable.canRemove(row, state) {
            parts.append("téléchargement incomplet, \(ModelTable.bytes(row.diskBytes)) sur le disque")
        }
        if row.active { parts.append(ModelTable.used(row)) }
        return parts.joined(separator: ", ")
    }
}

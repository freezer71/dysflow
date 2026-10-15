import Foundation

/// Ce que copie ⌘C : la sélection (si elle n'est pas vide), sinon la phrase
/// en cours ; et ce qu'on en dit.
enum ReadingCopy {
    static func choose(selection: String?, sentence: String) -> (text: String, message: String)? {
        if let selection, !selection.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return (selection, "Sélection copiée")
        }
        guard !sentence.isEmpty else { return nil }
        return (sentence, "Phrase copiée")
    }
}

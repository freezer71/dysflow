import Foundation

/// Écrit la dernière valeur d'une rafale de changements, une écriture à la
/// fois : un curseur qu'on glisse ne produit pas une écriture par pas.
@MainActor
final class Committer<Value: Equatable> {
    private let delay: Duration
    /// Reçoit la valeur à écrire et celle que le moteur a déjà ; rend celle
    /// qu'il a réellement appliquée (bornes comprises).
    private let write: (Value, Value) async throws -> Value
    private let onApplied: (_ sent: Value, _ applied: Value) -> Void
    private let onResult: (Error?) -> Void
    private var pending: Task<Void, Never>?
    private var inFlight: (id: Int, value: Value, job: Task<Void, Never>)?
    private var jobCounter = 0
    /// Change à chaque `prime` : ce qui date d'avant ne compte plus.
    private var epoch = 0
    /// La dernière valeur demandée, pour `flush`.
    private var latest: Value?
    /// La pause la plus longue demandée depuis la dernière écriture : une
    /// modification ordinaire ne raccourcit pas celle d'un réglage lourd.
    private var pendingWait: Duration = .zero
    /// Ce que le moteur a déjà : une valeur identique n'est pas renvoyée.
    private(set) var saved: Value?

    convenience init(delay: Duration = .milliseconds(150), saved: Value? = nil,
                     write: @escaping (Value) async throws -> Void,
                     onResult: @escaping (Error?) -> Void = { _ in }) {
        self.init(delay: delay, saved: saved,
                  writeApplied: { new, _ in try await write(new); return new },
                  onResult: onResult)
    }

    /// `writeChange` reçoit aussi la dernière valeur écrite : il peut
    /// n'envoyer que ce qui a changé.
    convenience init(delay: Duration = .milliseconds(150), saved: Value? = nil,
                     writeChange: @escaping (_ new: Value, _ previous: Value) async throws -> Void,
                     onResult: @escaping (Error?) -> Void = { _ in }) {
        self.init(delay: delay, saved: saved,
                  writeApplied: { new, previous in try await writeChange(new, previous); return new },
                  onResult: onResult)
    }

    init(delay: Duration = .milliseconds(150), saved: Value? = nil,
         writeApplied: @escaping (_ new: Value, _ previous: Value) async throws -> Value,
         onApplied: @escaping (_ sent: Value, _ applied: Value) -> Void = { _, _ in },
         onResult: @escaping (Error?) -> Void = { _ in }) {
        self.delay = delay
        self.saved = saved
        self.write = writeApplied
        self.onApplied = onApplied
        self.onResult = onResult
    }

    /// Fixe la valeur de départ (chargée du moteur) sans rien écrire. Tout ce
    /// qui avait été demandé avant (attente ou écriture en vol) est périmé :
    /// ni son écriture, ni son résultat ne touchent plus la référence.
    func prime(_ value: Value) {
        epoch += 1
        pending?.cancel()
        pending = nil
        pendingWait = .zero
        saved = value
        latest = value
    }

    /// `immediate` : sans pause (relâchement d'un curseur). `pause` : une pause
    /// plus longue que d'ordinaire (réglages qui rechargent la voix).
    func change(_ value: Value, immediate: Bool = false, after pause: Duration? = nil) {
        guard let saved else { return }   // pas encore chargé
        latest = value
        pending?.cancel()
        if value == saved && inFlight == nil { pending = nil; pendingWait = .zero; return }
        pendingWait = immediate ? .zero : max(pendingWait, pause ?? delay)
        let wait = pendingWait
        let myEpoch = epoch
        pending = Task {
            if wait > .zero {
                do { try await Task.sleep(for: wait) } catch { return }
            }
            // Une tâche annulée avant d'avoir tourné (sans pause, rien ne l'a
            // interrompue) ne touche pas à la pause de celle qui l'a remplacée.
            if Task.isCancelled || epoch != myEpoch { return }
            pendingWait = .zero
            // Une écriture en vol passe d'abord. Elle met à jour `saved` et vide
            // `inFlight` dans sa propre tâche : une fois attendue, tout est à jour.
            while let flying = inFlight {
                await flying.job.value
                if Task.isCancelled || epoch != myEpoch { return }
            }
            if Task.isCancelled || epoch != myEpoch { return }
            guard let previous = self.saved, value != previous else { return }
            jobCounter += 1
            let id = jobCounter
            let job = Task { @MainActor in
                do {
                    let applied = try await write(value, previous)
                    if epoch == myEpoch {
                        self.saved = applied
                        onApplied(value, applied)
                        onResult(nil)
                    }
                } catch {
                    if epoch == myEpoch { onResult(error) }
                }
                // Seulement si c'est encore la nôtre : une autre peut avoir pris la place.
                if inFlight?.id == id { inFlight = nil }
            }
            inFlight = (id: id, value: value, job: job)
            await job.value
        }
    }

    /// Écrit tout de suite ce qui attend sa pause, et rend la main une fois
    /// l'écriture faite : « Écouter » ne doit pas partir avant la voix choisie.
    func flush() async {
        if let latest, let saved, latest != saved {
            if inFlight?.value == latest {
                // L'écriture en vol porte déjà la dernière valeur : rien de plus,
                // et la pause de l'attente abandonnée ne vaut plus.
                pending?.cancel()
                pendingWait = .zero
            } else {
                change(latest, immediate: true)
            }
        }
        await pending?.value
        await inFlight?.job.value
    }
}

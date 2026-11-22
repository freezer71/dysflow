import Cmlx
@preconcurrency import MLX

/// La fin des calculs de mlx-swift, pour qui va rendre le verrou du GPU
/// (`MLXGate.shared`). Ajouté par Dysflow, hors des fichiers repris.
public enum MLXStreams {
    /// À appeler sur le fil qui a calculé : attend que tout ce qu'il a lancé
    /// soit fini (`asyncEval`, le pas de Nemotron, revient avant le GPU).
    ///
    /// Un fil ne peut lancer de calcul GPU que sur deux sortes de flux :
    /// - la paire de mlx-swift, une par processus (`Stream.gpu`, `Stream.cpu`),
    ///   qu'importe le fil qui a chargé le modèle ;
    /// - le flux par défaut de ce fil, que le C++ de MLX prend de lui-même
    ///   (`eval`, `compile`, ses opérateurs sans flux).
    /// Ce qui les nourrit d'un autre flux finit avant eux. L'app et le
    /// framework ne créent aucun autre flux : qui en créerait un (`Stream(…)`,
    /// `withNewDefaultStream`) devrait l'attendre lui-même.
    ///
    /// Sans effet tant que MLX n'a aucun flux : ni Metal ni les noyaux ne
    /// sont alors chargés. Une erreur du GPU arrête l'app, comme dans tout
    /// autre appel de mlx-swift.
    public static func synchronize() {
        guard hasStreams else { return }
        MLX.Stream.gpu.synchronize()
        _ = mlx_synchronize_default()
        MLX.Stream.cpu.synchronize()
    }

    private static var hasStreams: Bool {
        var all = mlx_vector_stream_new()
        defer { _ = mlx_vector_stream_free(all) }
        return mlx_get_streams(&all) == 0 && mlx_vector_stream_size(all) > 0
    }
}

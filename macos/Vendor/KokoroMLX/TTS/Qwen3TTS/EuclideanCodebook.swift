import Foundation
import MLX
import MLXNN

// MARK: - EuclideanCodebook

public final class EuclideanCodebook: Module {
    private let epsilon: Float = 1e-5
    private let dim: Int

    public var initialized: MLXArray
    public var embedding_sum: MLXArray
    public var cluster_usage: MLXArray

    public private(set) var _embedding: MLXArray
    public private(set) var _c2: MLXArray

    public init(dim: Int, codebookSize: Int) {
        self.dim = dim
        self.initialized = MLXArray.zeros([1], dtype: .float32)
        self.embedding_sum = MLXArray.zeros([codebookSize, dim], dtype: .float32)
        self.cluster_usage = MLXArray.zeros([codebookSize], dtype: .float32)

        let cluster_usageSafe = maximum(cluster_usage, epsilon).reshaped([codebookSize, 1])
        self._embedding = embedding_sum / cluster_usageSafe
        self._c2 = _embedding.square().sum(axis: -1) / 2
    }

    public func updateInPlace() {
        let cluster_usageSafe = maximum(cluster_usage, epsilon).reshaped([cluster_usage.shape[0], 1])
        _embedding = embedding_sum / cluster_usageSafe
        _c2 = _embedding.square().sum(axis: -1) / 2
    }

    override public func update(parameters: ModuleParameters, verify: Module.VerifyUpdate, path: [String] = [], modulePath: [String] = []) throws -> Self {
        try super.update(parameters: parameters, verify: verify, path: path, modulePath: modulePath)
        updateInPlace()
        return self
    }

    public func encode(_ xs: MLXArray) -> MLXArray {
        let targetShape = Array(xs.shape.dropLast())
        let flat = xs.reshaped([-1, dim])
        let dotProd = flat.matmul(swappedAxes(_embedding, -1, -2))
        let dists = _c2 - dotProd
        return argMin(dists, axis: -1).reshaped(targetShape)
    }

    public func decode(_ xs: MLXArray) -> MLXArray {
        let targetShape = xs.shape + [dim]
        let taken = take(_embedding, xs.flattened(), axis: 0)
        return taken.reshaped(targetShape)
    }
}

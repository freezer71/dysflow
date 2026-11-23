import XCTest
import MLX
@testable import KokoroMLX

/// P1 du port de Phonon-2 en Rust (procédure de port, tâche 16) : le Swift
/// actuel (`ParakeetModel.fromPhonon`, le chemin de l'app), lancé sur les
/// cinq entrées de Parakeet (`ParakeetFixtureTests.inputs`), écrit la
/// référence du port Rust, `tests/fixtures/mlx/phonon_2.json`, au format de
/// celles de Parakeet : texte, jetons, chaque pas du décodeur TDT, forme et
/// moyennes du mel et de la sortie de l'encodeur. Décodage glouton : aucune
/// graine.
///
/// Ignoré sans `DYSFLOW_WRITE_FIXTURES`. Il lui faut aussi
/// `DYSFLOW_STT_MODELS` (le dossier qui contient
/// `phonon_2/phonon-2.bps.tar.zst`) et `DYSFLOW_FIXTURE_COMMIT`. Pour
/// xcodebuild, chacun préfixé de `TEST_RUNNER_` :
///
///   TEST_RUNNER_DYSFLOW_WRITE_FIXTURES=1 TEST_RUNNER_DYSFLOW_STT_MODELS=… \
///   TEST_RUNNER_DYSFLOW_FIXTURE_COMMIT=$(git rev-parse HEAD) \
///   ./scripts/test-macos.sh PhononFixtureTests
final class PhononFixtureTests: XCTestCase {
    /// La révision épinglée du catalogue (`src/models.rs`) ; un essai Rust
    /// vérifie que la fixture a bien celle du catalogue.
    static let model = (id: "phonon_2", revision: "7e153e4054c0a10db6d47d6fff5c062d4c77c154")

    func testWritesThePhononFixture() throws {
        let env = ProcessInfo.processInfo.environment
        guard env["DYSFLOW_WRITE_FIXTURES"] != nil else {
            throw XCTSkip("DYSFLOW_WRITE_FIXTURES absent : la référence n'est pas réécrite")
        }
        let root = try XCTUnwrap(env["DYSFLOW_STT_MODELS"], "DYSFLOW_STT_MODELS : le dossier des modèles")
        let commit = try XCTUnwrap(env["DYSFLOW_FIXTURE_COMMIT"], "DYSFLOW_FIXTURE_COMMIT : le commit d'origine")
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let inputs = try ParakeetFixtureTests.inputs(repo)

        try autoreleasepool {
            let model = try ParakeetModel.fromPhonon(URL(fileURLWithPath: root).appendingPathComponent(Self.model.id))
            var cases: [ParakeetFixtureTests.Case] = []
            for input in inputs {
                cases.append(try ParakeetFixtureTests.run(model, input.name, input.samples))
            }
            let fixture = ParakeetFixtureTests.Fixture(
                model: Self.model.id, revision: Self.model.revision, commit: commit,
                generator: "macos/Tests/PhononFixtureTests.swift",
                compute_dtype: "\(model.computeDType)",
                quantization: "\(PhononContainer.format), groupes de \(FiveValueLinear.groupSize)", seed: nil,
                inputs: ParakeetFixtureTests.described(inputs), cases: cases)
            try ParakeetFixtureTests.write(
                fixture, to: repo.appendingPathComponent("tests/fixtures/mlx/\(Self.model.id).json"))
        }
        Memory.clearCache()
    }
}

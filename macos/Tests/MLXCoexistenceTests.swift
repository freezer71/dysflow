import XCTest
import MLX
@testable import Dysflow

/// Lot 0 des moteurs : MLX compilé par `mlx-rs` dans libdysflow_ffi.a et MLX
/// de mlx-swift (`Cmlx`) dans le même processus. Si les deux copies se
/// gênaient (symboles en double, allocateur Metal partagé), l'édition des
/// liens échouerait ou l'un des calculs se tromperait.
final class MLXCoexistenceTests: XCTestCase {
    func testRustAndSwiftMLXInOneProcess() throws {
        XCTAssertEqual(MLXArray([1, 2, 3] as [Float]).sum().item(Float.self), 6)
        XCTAssertEqual(try mlxRustSmoke(), 6)
        // Swift encore après Rust : le calcul Rust n'a rien cassé.
        XCTAssertEqual(MLXArray([1, 2, 3] as [Float]).sum().item(Float.self), 6)
    }
}

import XCTest
@testable import KinetoRuntime

/// Issue #233 · the artboard background a `.kvk` carries. The same four files
/// and expectations as `engine/kineto_engine/tests/background_parity.rs` and
/// `runtimes/parity/runtime_fixtures.json`; the Swift package must read the
/// straight RGBA the editor exported. All zeros means transparent.
final class KinetoBackgroundTests: XCTestCase {
    private func backgroundOf(_ name: String) throws -> [UInt8] {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: name, withExtension: "kvk"),
            "missing fixture \(name)"
        )
        let player = try KinetoPlayer(data: Data(contentsOf: url))
        let c = try player.backgroundColor()
        return [c.r, c.g, c.b, c.a]
    }

    func testHalfTransparentBackgroundIsStraightAlpha() throws {
        XCTAssertEqual(try backgroundOf("background_half"), [204, 51, 0, 128])
    }

    func testOpaqueBackground() throws {
        XCTAssertEqual(try backgroundOf("background_opaque"), [17, 34, 51, 255])
    }

    func testExcludedBackgroundIsTransparent() throws {
        XCTAssertEqual(try backgroundOf("background_off"), [0, 0, 0, 0])
    }

    func testLegacyFileWithoutTheFieldIsTransparent() throws {
        XCTAssertEqual(try backgroundOf("background_legacy"), [0, 0, 0, 0])
    }
}

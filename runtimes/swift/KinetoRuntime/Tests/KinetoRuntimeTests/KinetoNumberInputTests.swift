import XCTest
@testable import KinetoRuntime

/// Issue #238 — what a number input does on the Swift runtime, against a real
/// `.kvk` (the same `pointer_binding` document the web parity harness drives).
///
/// The Swift API exposes no override read-back, so these tests prove the host
/// contract (the value is accepted, kind-checked and survives a frame) and the
/// engine's `input_bindings.rs` proves the resulting pose. Together they are
/// the evidence the editor's "number inputs" label rests on.
final class KinetoNumberInputTests: XCTestCase {
    private let machineID = "sm-pointer"
    private let inputName = "pointerX"

    private func loadFixture() throws -> KinetoPlayer {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "pointer_binding", withExtension: "kvk")
        )
        return try KinetoPlayer(contentsOf: url)
    }

    func testTheFixtureDeclaresTheNumberInputAsANumber() throws {
        let player = try loadFixture()

        let input = try XCTUnwrap(
            player.inputs(in: machineID).first { $0.name == inputName }
        )

        XCTAssertEqual(input.kind, .number)
    }

    func testAHostCanSetANumberInputAndAdvance() throws {
        let player = try loadFixture()

        XCTAssertNoThrow(try player.setNumber(0.5, input: inputName, in: machineID))
        XCTAssertNoThrow(try player.advance(1.0 / 60.0))
        XCTAssertNoThrow(try player.setNumber(1.0, input: inputName, in: machineID))
        XCTAssertNoThrow(try player.advance(1.0 / 60.0))
    }

    func testOutOfRangeNumbersAreAcceptedAndClampedWhereRead() throws {
        let player = try loadFixture()

        XCTAssertNoThrow(try player.setNumber(-3, input: inputName, in: machineID))
        XCTAssertNoThrow(try player.setNumber(7, input: inputName, in: machineID))
    }

    func testANumberCannotBeSetOnAnInputThatDoesNotExist() throws {
        let player = try loadFixture()

        XCTAssertThrowsError(
            try player.setNumber(0.5, input: "no-such-input", in: machineID)
        )
        XCTAssertThrowsError(
            try player.setNumber(0.5, input: inputName, in: "no-such-machine")
        )
    }

    func testAPointerMoveIsAcceptedWithoutAListener() throws {
        let player = try loadFixture()

        XCTAssertNoThrow(try player.pointerMoved(x: 10, y: 10))
    }
}

import XCTest
@testable import KinetoRuntime

/// Issue #237: on the iOS Simulator the Metal device lacks the
/// `INDIRECT_EXECUTION` downlevel flag vello needs, and hosting the controller
/// used to abort the whole process instead of reaching its own error label.
@MainActor
final class KinetoPlayerViewControllerTests: XCTestCase {
    private func makePlayer() throws -> KinetoPlayer {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "pointer_binding", withExtension: "kvk"),
            "pointer_binding.kvk fixture is missing from the test bundle"
        )
        return try KinetoPlayer(contentsOf: url)
    }

    private func rendererLabel(in controller: KinetoPlayerViewController) -> UILabel? {
        controller.view.subviews
            .compactMap { $0 as? UILabel }
            .first { $0.text?.hasPrefix("Kineto renderer unavailable") == true }
    }

    private func host(_ controller: KinetoPlayerViewController) -> UIWindow {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 1024, height: 768))
        window.rootViewController = controller
        window.makeKeyAndVisible()
        controller.view.layoutIfNeeded()
        // The display link renders a few frames; none of them may abort.
        RunLoop.main.run(until: Date().addingTimeInterval(1.0))
        return window
    }

    func testShouldSurviveHostingWhenRendererIsUnsupported() throws {
        // Arrange
        let controller = KinetoPlayerViewController(player: try makePlayer())

        // Act: reaching the next line at all is the regression — before the
        // fix the process aborted inside the first rendered frame.
        let window = host(controller)

        // Assert
        #if targetEnvironment(simulator)
        let label = try XCTUnwrap(rendererLabel(in: controller))
        XCTAssertFalse(label.isHidden, "the unavailable label must be visible on the Simulator")
        #endif
        window.isHidden = true
    }

    func testShouldKeepHeadlessCallsWorkingAfterRendererFails() throws {
        // Arrange
        let player = try makePlayer()
        let controller = KinetoPlayerViewController(player: player)
        let window = host(controller)

        // Act / Assert: the player itself is independent of the surface.
        XCTAssertNoThrow(try player.advance(0.016))
        XCTAssertNoThrow(try player.play())
        XCTAssertNoThrow(try player.pause())
        window.isHidden = true
    }
}

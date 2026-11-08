import XCTest
@testable import KinetoRuntime

final class KinetoRuntimeTests: XCTestCase {
    func testInvalidDataThrowsLoadError() {
        do {
            _ = try KinetoPlayer(data: Data([0x00, 0x01, 0x02]))
            XCTFail("Expected invalid .kvk data to throw")
        } catch {
            let description = String(describing: error)

            XCTAssertTrue(description.contains("Kineto load failed"))
            XCTAssertTrue(description.contains("Unable to load .kvk data"))
        }
    }
}

import BaeKit
import Foundation
import Testing

/// uniffi gives `BridgeError` an `errorDescription` of `String(reflecting: self)`,
/// so `localizedDescription` on anything the bridge threw is a raw Rust enum
/// dump. These pin that a failure on its way to a human goes through core's keyed
/// line instead, and that the opaque detail survives for the copy button.
@Suite("DisplayError from any Error")
struct DisplayErrorTests {
    @Test("a bridge diagnostic renders its category's line, not the enum dump")
    func diagnosticRendersCategoryLine() {
        let error = BridgeError.Diagnostic(
            category: .database,
            detail: "no such table: albums"
        )

        let displayed = DisplayError(error as any Error)

        #expect(displayed.line == BridgeErrorCategory.database.localizedLine)
        // The whole point: the raw dump never reaches the user.
        #expect(!displayed.line.contains("BridgeError"))
        #expect(!displayed.line.contains("no such table"))
        // …but it is still there for "Copy Details".
        #expect(displayed.detail == "no such table: albums")
    }

    @Test("a not-found renders its entity's line and has no detail to disclose")
    func notFoundRendersEntityLine() {
        let error = BridgeError.NotFound(entity: .album, id: "album-1")

        let displayed = DisplayError(error as any Error)

        #expect(displayed.line == BridgeEntityKind.album.notFoundLine)
        #expect(!displayed.line.contains("BridgeError"))
        #expect(displayed.detail == nil)
    }

    /// The fallback is what lets a `catch` be routed without first proving what it
    /// can throw: a Swift-origin error keeps its own prose.
    @Test("a Swift-origin error passes through unchanged")
    func swiftErrorPassesThrough() {
        struct Failure: LocalizedError {
            var errorDescription: String? { "Couldn't read the file" }
        }
        let error = Failure()

        let displayed = DisplayError(error as any Error)

        #expect(displayed.line == "Couldn't read the file")
        #expect(displayed.detail == nil)
    }

    @Test("displayLine agrees with DisplayError for the same error")
    func displayLineAgrees() {
        let error: any Error = BridgeError.Diagnostic(
            category: .network,
            detail: "connection reset"
        )

        #expect(error.displayLine == DisplayError(error).line)
        #expect(!error.displayLine.contains("BridgeError"))
    }
}

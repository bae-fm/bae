import BaeKit
import Foundation
import Testing

/// uniffi gives `BridgeError` an `errorDescription` of `String(reflecting: self)`,
/// so `localizedDescription` on anything the bridge threw is a raw Rust enum
/// dump. These pin that a failure on its way to a human goes through core's keyed
/// line instead, that the opaque detail survives for the copy button, and that an
/// error core says has no line produces nothing at all rather than a blank one.
@Suite("DisplayError from any Error")
struct DisplayErrorTests {
    @Test("a bridge diagnostic renders its category's line, not the enum dump")
    func diagnosticRendersCategoryLine() throws {
        let error = BridgeError.Diagnostic(
            category: .database,
            detail: "no such table: albums"
        )

        let displayed = try #require(DisplayError(error as any Error))

        #expect(displayed.line == BridgeErrorCategory.database.localizedLine)
        // The whole point: the raw dump never reaches the user.
        #expect(!displayed.line.contains("BridgeError"))
        #expect(!displayed.line.contains("no such table"))
        // …but it is still there for "Copy Details".
        #expect(displayed.detail == "no such table: albums")
    }

    @Test("a not-found renders its entity's line and has no detail to disclose")
    func notFoundRendersEntityLine() throws {
        let error = BridgeError.NotFound(entity: .album, id: "album-1")

        let displayed = try #require(DisplayError(error as any Error))

        #expect(displayed.line == BridgeEntityKind.album.notFoundLine)
        #expect(!displayed.line.contains("BridgeError"))
        #expect(displayed.detail == nil)
    }

    /// The user cancelled. That says nothing back to them, so there is nothing to
    /// display — not an empty line, which is what used to open a blank alert.
    @Test("a cancellation produces no error to display")
    func cancelledProducesNothing() {
        #expect(DisplayError(BridgeError.Cancelled as any Error) == nil)
        #expect((BridgeError.Cancelled as any Error).displayLine == nil)
        #expect(BridgeError.Cancelled.localizedLine == nil)
    }

    /// The fallback is what lets a `catch` be routed without first proving what it
    /// can throw: a Swift-origin error keeps its own prose.
    @Test("a Swift-origin error passes through unchanged")
    func swiftErrorPassesThrough() throws {
        struct Failure: LocalizedError {
            var errorDescription: String? { "Couldn't read the file" }
        }

        let displayed = try #require(DisplayError(Failure() as any Error))

        #expect(displayed.line == "Couldn't read the file")
        #expect(displayed.detail == nil)
    }

    @Test("displayLine agrees with DisplayError for the same error")
    func displayLineAgrees() throws {
        let error: any Error = BridgeError.Diagnostic(
            category: .network,
            detail: "connection reset"
        )

        let line = try #require(error.displayLine)

        #expect(line == DisplayError(error)?.line)
        #expect(!line.contains("BridgeError"))
    }

    @Test("context preserves the typed error line and diagnostic detail")
    func contextPreservesTypedError() throws {
        let displayed = try #require(
            DisplayError(
                BridgeError.Diagnostic(
                    category: .network,
                    detail: "connection reset"
                ) as any Error
            )
        )

        #expect(
            displayed.addingContext("Downloads (/Volumes/Music)").line
                == "Downloads (/Volumes/Music): \(displayed.line)"
        )
        #expect(
            displayed.addingContext("Downloads (/Volumes/Music)").detail
                == "connection reset"
        )
    }

    @Test(
        "long diagnostics expose a bounded opening excerpt and retain the full detail"
    )
    func longDiagnosticExcerpt() throws {
        let detail =
            String(repeating: "opening context ", count: 40)
            + "terminal context"
        let displayed = try #require(
            DisplayError(
                BridgeError.Diagnostic(category: .database, detail: detail)
                    as any Error
            )
        )
        let excerpt = try #require(displayed.detailExcerpt)

        #expect(excerpt.hasPrefix("opening context"))
        #expect(excerpt.hasSuffix("…"))
        #expect(excerpt.count < detail.count)
        #expect(displayed.detail == detail)
    }
}

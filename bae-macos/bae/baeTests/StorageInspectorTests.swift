import BaeKit
import Testing

@testable import bae

/// Which files and transfers the inspector shows is core's, read for the
/// release it is set to (bae-core's storage inspector tests); this covers
/// only which release it is set to.
@MainActor
@Suite("Storage inspector")
struct StorageInspectorTests {
    @Test("inspector requires exactly one selected release")
    func inspectorRequiresExactlyOneSelectedRelease() {
        #expect(StorageInspector.releaseId(in: []) == nil)
        #expect(
            StorageInspector.releaseId(in: ["rel-selected"])
                == "rel-selected"
        )
        #expect(
            StorageInspector.releaseId(
                in: ["rel-selected", "rel-other"]
            ) == nil
        )
    }
}

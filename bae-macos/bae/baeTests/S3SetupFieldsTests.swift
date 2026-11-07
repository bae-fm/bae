import BaeKit
import Testing

@testable import bae

/// A failed S3 setup's reason sits under the field it names, and any other
/// failure is left for the step to show at the top.
@MainActor
@Suite("S3 setup fields")
struct S3SetupFieldsTests {
    private func failure(_ failure: BridgeStorageFailure) -> BridgeError {
        .Diagnostic(category: .storage(failure: failure), detail: "")
    }

    @Test(
        "region, missing bucket and occupied bucket each sit under their field"
    )
    func placesByReason() {
        let cases: [(BridgeStorageFailure, S3SetupFields.Field)] = [
            (.regionMismatch, .region),
            (.containerNotFound, .bucket),
            (.ownLocationRequired, .bucket),
        ]
        for (reason, field) in cases {
            let fields = S3SetupFields()
            #expect(fields.place(failure(reason)))
            #expect(fields.fieldFailure?.field == field)
            fields.clearFailure()
            #expect(fields.fieldFailure == nil)
        }
    }

    @Test("any other reason is not placed")
    func leavesTheRest() {
        let fields = S3SetupFields()
        #expect(!fields.place(failure(.authentication)))
        #expect(!fields.place(failure(.permissionDenied)))
        #expect(fields.fieldFailure == nil)
    }
}

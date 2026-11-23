import AppKit
import ApplicationServices
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct MacS3FormAccessibilityTests {
    @Test(arguments: [false, true])
    func readsEachFieldWithItsLabelAndError(hasError: Bool) async throws {
        let fields = S3SetupFields()
        fields.endpoint = "https://s3.home.lan"
        fields.region = "us-east-1"
        fields.bucket = "music"
        fields.accessKey = "access-key"
        fields.secretKey = "secret-key"
        if hasError {
            try #require(
                fields.place(
                    BridgeError.Diagnostic(
                        category: .storage(failure: .regionMismatch),
                        detail: "the endpoint rejected the region"
                    )
                )
            )
        }

        try await SnapshotTestSupport.withHostedWindow(
            MacS3Form(staged: fields)
                .environment(Sync.stub())
                .environment(StorageSetupRun()),
            size: NSSize(width: 520, height: 470)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "S3 setup"
            )
            let controls = tree.nodes.filter { $0.role == kAXTextFieldRole }
            #expect(
                controls.map(\.controlName) == [
                    "Endpoint", "Region", "Bucket", "Access key ID",
                    "Secret access key",
                ]
            )
            let region = try #require(
                controls.first {
                    $0.controlName == "Region"
                }
            )
            let row = try #require(region.parent)
            let parts = tree.nodes.filter { $0.parent == row }
            let expectedRoles =
                hasError
                ? [kAXStaticTextRole, kAXTextFieldRole, kAXStaticTextRole]
                : [kAXStaticTextRole, kAXTextFieldRole]
            #expect(parts.map(\.role) == expectedRoles)
            if case .text(let label)? = parts.first?.value {
                #expect(label == "Region")
            }
            else {
                Issue.record("The region row has no text label.")
            }
        }
    }
}

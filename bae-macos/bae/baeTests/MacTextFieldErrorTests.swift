import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct MacTextFieldErrorTests {
    /// A message from core or a cloud provider is text, not a localized key:
    /// SwiftUI reads a key for markdown and format specifiers.
    @Test(arguments: [
        "Upload 100% failed: bucket %@ not found",
        "Region *us-east-1* is wrong for [this bucket](https://example.com)",
        "Request %d of %s was refused",
    ])
    func showsAnErrorLineUnchanged(message: String) async throws {
        try await SnapshotTestSupport.withHostedWindow(
            FieldColumn {
                MacTextField("Region", error: message) {
                    BoxedField(text: .constant("us-east-1"), invalid: true)
                }
            }
            .frame(width: 400),
            size: NSSize(width: 400, height: 120)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "Text field error"
            )
            #expect(tree.nodes.compactMap(\.staticText).contains(message))
        }
    }
}

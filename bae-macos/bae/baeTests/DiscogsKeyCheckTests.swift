import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Checking a Discogs key asks Discogs over the network, so the key's row
/// says so while it does.
@MainActor
@Suite("Discogs key check")
struct DiscogsKeyCheckTests {
    @Test(
        "a running check is named",
        arguments: [
            BridgeDiscogsTokenStatus.notConfigured, .unvalidated,
        ]
    )
    func checkIsNamed(status: BridgeDiscogsTokenStatus) async throws {
        let size = NSSize(width: 500, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            Form {
                Section {
                    DiscogsSettingsContent(
                        draft: .constant("abcdef123456"),
                        status: status,
                        isValidating: true,
                        saveError: nil,
                        readError: nil,
                        onSave: {},
                        onRecheck: {},
                        onRemove: {}
                    )
                }
            }
            .formStyle(.grouped)
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(
                lines.carrying("Checking the key with Discogs"),
                "drew \(lines)"
            )
        }
    }
}

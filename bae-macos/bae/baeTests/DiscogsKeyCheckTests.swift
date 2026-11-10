import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Checking a Discogs key asks Discogs over the network, so the key's row
/// spins, saying so, while it does; the words are the
/// `settings-discogs-checking` shot's.
@MainActor
@Suite("Discogs key check")
struct DiscogsKeyCheckTests {
    @Test(
        "the key's row spins only while a check runs",
        arguments: [
            BridgeDiscogsTokenStatus.notConfigured, .unvalidated, .rejected,
        ],
        [true, false]
    )
    func spinsWhileChecking(
        status: BridgeDiscogsTokenStatus,
        isValidating: Bool
    ) async throws {
        let size = NSSize(width: 500, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            Form {
                Section {
                    DiscogsSettingsContent(
                        draft: .constant("abcdef123456"),
                        status: status,
                        isEnabled: true,
                        isValidating: isValidating,
                        wasRejected: false,
                        saveError: nil,
                        readError: nil,
                        onSave: {},
                        onRecheck: {}
                    )
                }
            }
            .formStyle(.grouped)
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.count == (isValidating ? 1 : 0))
        }
    }
}

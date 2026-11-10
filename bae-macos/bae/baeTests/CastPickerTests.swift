import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Connecting to a device can take a while, so the picker spins, naming the
/// device it is connecting to, until the cast starts; the words are the
/// `cast-picker-connecting` shot's.
@MainActor
@Suite("Cast picker")
struct CastPickerTests {
    @Test(
        "the picker spins only while a cast connects",
        arguments: ["Kitchen", nil]
    )
    func spinsWhileConnecting(connectingName: String?) async throws {
        let size = NSSize(width: 300, height: 160)
        try await SnapshotTestSupport.withHostedWindow(
            CastPickerPopover(
                receivers: BridgeCastReceiverList(
                    casting: nil,
                    found: [
                        BridgeCastReceiver(
                            id: "speaker-1",
                            name: "Kitchen",
                            kind: .cast
                        )
                    ]
                ),
                endedFailure: nil,
                connectingName: connectingName,
                onDisconnect: {}
            )
            .environment(CastStart.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.count == (connectingName == nil ? 0 : 1))
        }
    }
}

import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Connecting to a device can take a while, so the row of the device the
/// picker is connecting to spins until the cast starts.
@MainActor
@Suite("Cast picker")
struct CastPickerTests {
    nonisolated private static let kitchen = BridgeCastReceiver(
        id: "speaker-1",
        name: "Kitchen",
        kind: .airPlay,
        model: nil,
        address: "192.168.1.41",
        takesVolume: true
    )

    @Test(
        "the picker spins only while a cast connects",
        arguments: [
            CastStart.Connecting(receiver: kitchen, ownerDeciding: false),
            CastStart.Connecting(receiver: kitchen, ownerDeciding: true),
            nil,
        ]
    )
    func spinsWhileConnecting(connecting: CastStart.Connecting?) async throws {
        let size = NSSize(width: 300, height: 160)
        try await SnapshotTestSupport.withHostedWindow(
            MacCastPopover(
                receivers: BridgeCastReceiverList(
                    casting: nil,
                    found: [Self.kitchen],
                    isEmpty: false
                ),
                endedFailure: nil,
                connecting: connecting,
                onDisconnect: {}
            )
            .environment(CastStart.stub())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.count == (connecting == nil ? 0 : 1))
        }
    }
}

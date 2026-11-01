import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Connecting to a device can take a while, so the picker names the device
/// it is connecting to until the cast starts.
@MainActor
@Suite("Cast picker")
struct CastPickerTests {
    @Test("a connecting cast names its device")
    func connectingNamesTheDevice() async throws {
        let size = NSSize(width: 300, height: 160)
        try await SnapshotTestSupport.withHostedWindow(
            CastPickerPopover(
                devices: [
                    BridgeCastDevice(
                        id: "speaker-1",
                        name: "Kitchen",
                        kind: .cast
                    )
                ],
                castingDevice: nil,
                endedFailure: nil,
                connectingName: "Kitchen",
                onCast: { _ in },
                onDisconnect: {}
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(lines.carrying("Connecting to Kitchen"), "drew \(lines)")
        }
    }
}

import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The order the now playing bar's controls are read in, as the board lists
/// them: the track, the transport, the seek bar, then Play on, Queue, Mute
/// and Volume.
/// Read through the accessibility API, as a screen reader reads: SwiftUI
/// builds its elements only for such a client.
@MainActor
@Suite("Now-playing bar reading order")
struct NowPlayingBarOrderTests {
    @Test(
        "the seek bar is read after the transport and before the right controls"
    )
    func seekBarBeforeRightControls() async throws {
        let size = NSSize(width: 1_200, height: 140)
        let cast = CastButton()
            .environment(Cast.stub())
            .environment(CastStore())
            .environment(CastStart.stub())
        let bar = NowPlayingBarFixture.make(
            settings: PlaybackSettings(
                volume: 0.7,
                isMuted: false,
                repeatMode: .off
            ),
            castControl: AnyView(cast)
        )
        let nodes = try await SnapshotTestSupport.withHostedWindow(
            VStack {
                Text("Library")
                bar.environment(PreviewData.artImageStore())
            }
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            return
                try AccessibilityTree.read(
                    of: window,
                    subject: "the now playing bar"
                )
                .nodes
        }
        let labels = nodes.map { $0.description ?? $0.title ?? "" }
        func index(_ words: String, role: NSAccessibility.Role? = nil) throws
            -> Int
        {
            try #require(
                nodes.indices.first { index in
                    (role == nil || nodes[index].role == role?.rawValue)
                        && labels[index].localizedCaseInsensitiveContains(words)
                },
                "listed \(labels)"
            )
        }
        let library = try index("Library")
        let repeatControl = try index("Repeat")
        let seek = try index("position", role: .slider)
        let castControl = try index("Play on")
        let queue = try index("Queue")
        let mute = try index("Mute")
        let volume = try index("Volume")
        #expect(library < repeatControl, "listed \(labels)")
        #expect(repeatControl < seek, "listed \(labels)")
        #expect(seek < castControl, "listed \(labels)")
        #expect(castControl < queue, "listed \(labels)")
        #expect(queue < mute, "listed \(labels)")
        #expect(mute < volume, "listed \(labels)")
    }
}

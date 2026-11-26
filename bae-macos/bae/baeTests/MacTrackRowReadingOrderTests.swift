import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A track row's pieces read in the order the board draws them: number,
/// title, artists, length. Read through the accessibility API, as a screen
/// reader reads: SwiftUI builds its elements only for such a client.
@MainActor
@Suite("Track row reading order")
struct MacTrackRowReadingOrderTests {
    @Test("a row with artists reads number, title, artists, length")
    func compilationRow() async throws {
        let size = NSSize(width: 840, height: 52)
        let texts = try await SnapshotTestSupport.withHostedWindow(
            MacTrackRow(
                track: PreviewData.previewTrack(
                    title: "Low Tide Hours",
                    position: "4",
                    durationMs: 303_000,
                    displayArtist: "Mara Quill"
                ),
                artist: "Mara Quill",
                positionWidth: MacTrackRow.leadingSlotMinWidth,
                isCurrent: false,
                isPlaying: false,
                rating: .hidden,
                onPlay: {},
                onTogglePlayPause: {}
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "a track row"
            )
            return tree.nodes.compactMap(\.textWords)
        }
        #expect(
            texts == ["4", "Low Tide Hours", "Mara Quill", "5:03"],
            "read \(texts)"
        )
    }
}

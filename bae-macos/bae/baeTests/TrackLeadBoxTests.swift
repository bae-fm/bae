import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct TrackLeadBoxTests {
    private func row(current: Bool, hover: Bool) -> some View {
        let track = PreviewData.previewTrack(
            title: "Low Tide Hours",
            position: "4",
            durationMs: 303_000
        )
        return MacTrackRow(
            track: track,
            artist: nil,
            positionWidth: MacTrackRow.leadingSlotMinWidth,
            isCurrent: current,
            isPlaying: current,
            hover: hover,
            rating: .hidden,
            onPlay: {},
            onTogglePlayPause: {}
        )
        .frame(width: 840, height: 40)
    }

    @Test("Play, Pause and the speaker each give VoiceOver their own box")
    func glyphsHaveTheirOwnBoxes() async throws {
        let size = NSSize(width: 840, height: 200)
        try await SnapshotTestSupport.withHostedWindow(
            VStack(alignment: .leading, spacing: 20) {
                row(current: false, hover: true)
                row(current: true, hover: false)
                row(current: true, hover: true)
            }
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tree = try AccessibilityTree.read(
                of: window,
                subject: "the track rows"
            )
            let glyphs = ["Play", "Now playing", "Pause"]
                .compactMap { name in
                    tree.nodes.first { $0.description == name }
                }
            #expect(glyphs.count == 3)
            // A box all three share is the lead's, not a glyph's. Two glyphs
            // can measure the same: Play and Pause are half a point apart in
            // width, which a display's pixel grid can round away.
            let boxes = Set(glyphs.map { "\($0.width) by \($0.height)" })
            #expect(boxes.count > 1)
        }
    }
}

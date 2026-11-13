import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Album track list layout")
@MainActor
struct AlbumTrackListLayoutTests {
    /// A side over eight tracks splits across two columns and a shorter one
    /// stays in the first, each row's play button marking where its row
    /// starts. That every row is a column wide, so a short side's durations
    /// end where the split side's first column does, is the
    /// `album-detail-mixed-columns` shot's; that a position as wide as "A10"
    /// stays on one line is the `album-track-list-wide-positions` shot's.
    @Test(
        "A side over eight tracks splits into two columns; a shorter one does not"
    )
    func sidesSplitIntoColumns() async throws {
        let release = PreviewData.releaseDetail(albumId: "a-23")
        #expect(release.trackGroups.map(\.tracks.count) == [9, 8])

        let size = NSSize(width: 800, height: 900)
        try await withHostedTrackList(release: release, size: size) { _, host in
            try await SnapshotTestSupport.settle(host)

            // Nine tracks split five over four; the eight-track side stays whole.
            // Both of the first column's runs share one x, ordered down the pane.
            let bands = rowBands(in: host, release: release)
            let columnX = try #require(bands.map(\.x).min())
            let secondColumnX = try #require(bands.map(\.x).max())
            #expect(secondColumnX - columnX > 100)
            let firstColumn = bands.filter { $0.x < columnX + 20 }
                .sorted { $0.top < $1.top }
            let secondColumn = bands.filter { $0.x > columnX + 20 }
                .sorted { $0.top < $1.top }
            #expect(firstColumn.count == 13)
            #expect(secondColumn.count == 4)
        }
    }

    /// The track list over the pane's background, with nothing playing.
    private func withHostedTrackList<Value>(
        release: ReleaseDetail,
        size: NSSize,
        _ body: (NSWindow, NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            AlbumTrackListView(
                release: release,
                isCompilation: false,
                currentTrackId: nil,
                isPlaying: false,
                onPlayFromTrack: { _ in },
                onTogglePlayPause: {},
                onAddNext: { _ in },
                onAddToQueue: { _ in },
                onExportTrack: { _ in },
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Theme.background)
            .environment(UiStore()),
            size: size
        ) {
            try await body($0, $1)
        }
    }

    /// A band per row: where its play button is laid out. SwiftUI puts a
    /// stand-in view for each focusable control into the hosting view, at the
    /// control's frame, so the hosting view holds each row's button once. A
    /// row's button stays in the layout tree whatever the row shows,
    /// opacity-toggled, centred in its leading slot.
    private func rowBands(
        in host: NSView,
        release: ReleaseDetail
    ) -> [RowBand] {
        let frames = SnapshotTestSupport.descendants(of: host)
            .filter { String(describing: type(of: $0)) == "KeyViewProxy" }
            .map { $0.convert($0.bounds, to: host) }
        let bands = frames.map { frame in
            RowBand(
                x: frame.minX,
                width: frame.width,
                top: host.isFlipped
                    ? frame.minY : host.bounds.height - frame.maxY,
                bottom: host.isFlipped
                    ? frame.maxY : host.bounds.height - frame.minY
            )
        }
        #expect(bands.count == release.tracks.count)
        return bands
    }

    /// One row's vertical extent and leading edge, in the host's points with y
    /// growing downward.
    private struct RowBand {
        let x: CGFloat
        let width: CGFloat
        let top: CGFloat
        let bottom: CGFloat
    }
}

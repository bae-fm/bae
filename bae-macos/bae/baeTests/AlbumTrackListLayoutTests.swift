import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Album track list layout")
@MainActor
struct AlbumTrackListLayoutTests {
    /// Every row in a release's track list is laid out over the same column
    /// width, whatever the shape of the side it belongs to. A row fills the
    /// width it is proposed — its duration sits at the trailing edge of its
    /// column — so a side short enough to stay one column has to be proposed
    /// a column's width, not the whole pane's, or its durations land hundreds
    /// of points right of the split side's above it.
    @Test("A short side's durations end where a split side's first column does")
    func shortSideEndsWhereTheSplitSideDoes() async throws {
        let release = PreviewData.releaseDetail(albumId: "a-23")
        #expect(release.trackGroups.map(\.tracks.count) == [9, 8])

        let size = NSSize(width: 800, height: 900)
        let (window, host) = hostTrackList(release: release, size: size)
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: .darkAqua)
        defer {
            window.contentView = nil
            window.close()
        }
        await SnapshotTestSupport.settle(host)

        // Nine tracks split five over four; the eight-track side stays whole.
        // Both of the first column's runs share one x, ordered down the pane.
        let bands = try rowBands(in: host, height: size.height)
        let columnX = try #require(bands.map(\.x).min())
        let secondColumnX = try #require(bands.map(\.x).max())
        #expect(secondColumnX - columnX > 100)
        let firstColumn = bands.filter { $0.x < columnX + 20 }
            .sorted { $0.top < $1.top }
        let secondColumn = bands.filter { $0.x > columnX + 20 }
            .sorted { $0.top < $1.top }
        #expect(firstColumn.count == 13)
        #expect(secondColumn.count == 4)

        let pane = try await Pane.capture(
            host,
            size: size,
            secondColumn: secondColumn,
            secondColumnX: secondColumnX
        )
        let splitSideEnd = try pane.columnEnd(of: firstColumn.prefix(5))
        let shortSideEnd = try pane.columnEnd(of: firstColumn.dropFirst(5))
        #expect(
            abs(shortSideEnd - splitSideEnd) < 2,
            "short side ends at \(shortSideEnd), split side at \(splitSideEnd)"
        )
        // The pin on the two-column case: its second column still runs to the
        // pane's trailing edge, so nothing passes by narrowing every row.
        let secondColumnEnd = try pane.columnEnd(of: secondColumn)
        #expect(
            secondColumnEnd > size.width - 20,
            "second column ends at \(secondColumnEnd)"
        )
    }

    /// The track list over the pane's background, with nothing playing and
    /// nothing loading, in a key window the capture can read pixels back from.
    private func hostTrackList(
        release: ReleaseDetail,
        size: NSSize
    ) -> (window: NSWindow, host: NSView) {
        SnapshotTestSupport.hostInWindow(
            AlbumTrackListView(
                release: release,
                isCompilation: false,
                currentTrackId: nil,
                loadingTrackId: nil,
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
        )
    }

    /// A band per row. Every row keeps its loading spinner in the layout tree,
    /// opacity-toggled, so the hosted tree carries one `NSProgressIndicator`
    /// per row wherever the row was laid out.
    private func rowBands(in host: NSView, height: CGFloat) throws
        -> [RowBand]
    {
        let indicators = SnapshotTestSupport.descendants(of: host)
            .filter { $0 is NSProgressIndicator }
        let bands = indicators.map { indicator -> RowBand in
            let frame = indicator.convert(indicator.bounds, to: host)
            return RowBand(
                x: frame.minX,
                top: host.isFlipped ? frame.minY : height - frame.maxY,
                bottom: host.isFlipped ? frame.maxY : height - frame.minY
            )
        }
        #expect(bands.count == 17)
        return bands
    }

    /// One row's vertical extent and leading edge, in the host's points with y
    /// growing downward.
    private struct RowBand {
        let x: CGFloat
        let top: CGFloat
        let bottom: CGFloat
    }

    /// The rendered track list read back as pixels: where a column's durations
    /// end is a question about the ink standing over the background.
    private struct Pane {
        let image: NSBitmapImageRep
        let background: NSColor
        let width: CGFloat
        /// The two-column groups' second column, which bounds how far right a
        /// row beside one may be scanned.
        let secondColumn: [RowBand]
        let secondColumnX: CGFloat

        @MainActor
        static func capture(
            _ host: NSView,
            size: NSSize,
            secondColumn: [RowBand],
            secondColumnX: CGFloat
        ) async throws -> Pane {
            let image = try await bitmap(host, size: size)
            // Past the list's last row, so it carries the pane's own colour.
            let background = try #require(
                image.colorAt(x: image.pixelsWide - 4, y: image.pixelsHigh - 4)
            )
            return Pane(
                image: image,
                background: background,
                width: size.width,
                secondColumn: secondColumn,
                secondColumnX: secondColumnX
            )
        }

        /// Where a column's durations end: the rightmost of its rows' last
        /// glyphs, since a glyph's own right sidebearing puts each row's ink
        /// up to a point short of the column's edge.
        func columnEnd(of bands: some Collection<RowBand>) throws -> CGFloat {
            try #require(bands.map(durationEnd).max())
        }

        /// A row beside a second column may only be scanned as far as that
        /// column's leading edge, so its neighbour's ink is never mistaken for
        /// its own duration.
        private func durationEnd(of band: RowBand) throws -> CGFloat {
            let hasNeighbour = secondColumn.contains {
                $0.x > band.x + 20 && $0.top < band.bottom
                    && band.top < $0.bottom
            }
            return try rightmostInk(
                band: band,
                toX: hasNeighbour ? secondColumnX : width
            )
        }

        /// A pixel counts as ink when it differs from the background by more
        /// than this. Glyphs stand far above it; a row's hover fill does not:
        /// that fill is `Color.primary` at 5% opacity, bleeding 10pt past the
        /// column edge, and on a CI runner whose cursor rests over the window
        /// one row is always hovered. Measuring it as ink put that row's end
        /// 10pt right of its neighbours'.
        private static let inkThreshold: CGFloat = 0.2

        /// The x of the rightmost glyph pixel across the row's vertical
        /// middle, from the row's own leading edge to `toX`.
        private func rightmostInk(band: RowBand, toX: CGFloat) throws
            -> CGFloat
        {
            let scale = CGFloat(image.pixelsWide) / image.size.width
            let center = (band.top + band.bottom) / 2
            let firstRow = max(Int((center - 6) * scale), 0)
            let lastRow = min(Int((center + 6) * scale), image.pixelsHigh - 1)
            let firstColumn = max(Int(band.x * scale), 0)
            let lastColumn = min(Int(toX * scale) - 1, image.pixelsWide - 1)
            var rightmost: Int?
            for y in firstRow...lastRow {
                for x in stride(from: lastColumn, through: firstColumn, by: -1)
                {
                    if x <= (rightmost ?? Int.min) { break }
                    guard let pixel = image.colorAt(x: x, y: y) else {
                        continue
                    }
                    if distance(pixel, background) > Self.inkThreshold {
                        rightmost = x
                        break
                    }
                }
            }
            return CGFloat(try #require(rightmost)) / scale
        }

        @MainActor
        private static func bitmap(_ host: NSView, size: NSSize) async throws
            -> NSBitmapImageRep
        {
            await SnapshotTestSupport.settle(host)
            try await Task.sleep(for: .milliseconds(250))
            let bitmap = try #require(
                NSBitmapImageRep(
                    bitmapDataPlanes: nil,
                    pixelsWide: Int(size.width) * 2,
                    pixelsHigh: Int(size.height) * 2,
                    bitsPerSample: 8,
                    samplesPerPixel: 4,
                    hasAlpha: true,
                    isPlanar: false,
                    colorSpaceName: .deviceRGB,
                    bytesPerRow: 0,
                    bitsPerPixel: 0
                )?
                .retagging(with: .sRGB)
            )
            bitmap.size = size
            host.cacheDisplay(in: NSRect(origin: .zero, size: size), to: bitmap)
            return bitmap
        }

        private func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
            max(
                abs(a.redComponent - b.redComponent),
                abs(a.greenComponent - b.greenComponent),
                abs(a.blueComponent - b.blueComponent)
            )
        }
    }
}

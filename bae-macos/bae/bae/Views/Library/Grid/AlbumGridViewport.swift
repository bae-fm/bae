import CoreGraphics

/// A grid slot the scroll can keep on top.
enum AlbumGridSlot: Hashable {
    /// The album slot at a list position.
    case position(Int)
    /// The open album's detail.
    case detail(albumId: String)
}

/// Where the grid's scroll view stands: its content offset, the height it
/// shows, and the height of its content.
struct AlbumGridScroll: Equatable {
    var offset: CGFloat = 0
    var visibleHeight: CGFloat = 0
    var contentHeight: CGFloat = 0
}

/// The track row and scroll bounds measured in one content layout.
struct PlacedTrackRow: Equatable {
    let trackId: String
    let seq: Int
    let frame: CGRect
    let scroll: AlbumGridScroll

    /// The offset showing the whole row, or nil if it is already visible.
    var scrollOffset: CGFloat? {
        let height = scroll.visibleHeight
        if frame.minY > scroll.offset - 0.5,
            frame.maxY < scroll.offset + height + 0.5
        {
            return nil
        }
        let target =
            frame.height > height ? frame.minY : frame.midY - height / 2
        return min(max(target, 0), max(scroll.contentHeight - height, 0))
    }
}

/// Where the grid's slots sit in the visible area, the slot a change of
/// column count keeps on top, and where the row a reveal scrolls to is.
///
/// A reference the view holds rather than view state: slots report their
/// frames on every scrolled frame, and nothing the grid draws reads them.
@MainActor
final class AlbumGridViewport {
    // periphery:ignore - a dictionary key: hashed and compared, never read.
    /// One shown cell at one column count. A position's placeholder and its
    /// album are different cells, so the one leaving never takes the other's
    /// frame with it.
    private struct Placement: Hashable {
        let cell: AlbumGridCell.Identity
        let columnCount: Int
    }

    /// Each shown cell's slot and frame in the visible area, by the column
    /// count it was laid out at, so a layout at a new count never reads as
    /// the one before it.
    private var frames: [Placement: (slot: AlbumGridSlot, frame: CGRect)] =
        [:]
    private var scroll = AlbumGridScroll()
    /// The slot the last change of column count put on top, held there
    /// until the grid scrolls another way, so changes back and forth return
    /// to the same place.
    private(set) var held: AlbumGridSlot?
    /// The row of the track the pending reveal names, while it is laid out.
    private var revealRow: PlacedTrackRow?
    /// The reveal whose scroll to its album has come to rest, so its row's
    /// place is measured where the scroll stops, not on the way there.
    private var revealAtAlbum: Int?

    func place(
        _ cell: AlbumGridCell.Identity,
        as slot: AlbumGridSlot,
        columnCount: Int,
        frame: CGRect
    ) {
        frames[Placement(cell: cell, columnCount: columnCount)] = (slot, frame)
    }

    func remove(_ cell: AlbumGridCell.Identity, columnCount: Int) {
        frames.removeValue(
            forKey: Placement(cell: cell, columnCount: columnCount)
        )
    }

    func setScroll(_ scroll: AlbumGridScroll) {
        self.scroll = scroll
    }

    func placeRevealedRow(_ row: PlacedTrackRow?) {
        revealRow = row
    }

    /// Records that reveal `seq` has scrolled to its album.
    func revealReachedAlbum(seq: Int) {
        revealAtAlbum = seq
    }

    /// The row reveal `seq` names, laid out; `atAlbum` asks only once the
    /// reveal has scrolled to its album.
    func revealedRow(seq: Int, atAlbum: Bool = false) -> PlacedTrackRow? {
        guard let revealRow, revealRow.seq == seq,
            !atAlbum || revealAtAlbum == seq
        else { return nil }
        return revealRow
    }

    /// Ends the hold, for a scroll or a change in what the grid shows; the
    /// next change of column count keeps whatever is on top then.
    func release() {
        held = nil
    }

    /// The slot to keep on top as the column count goes from `old` to `new`:
    /// the one held from an earlier change, or the top-most slot at `old`
    /// with at least half of it in view, leftmost in its row. Frames laid out
    /// at other counts are dropped.
    func anchor(from old: Int, to new: Int) -> AlbumGridSlot? {
        let anchor = held ?? topSlot(columnCount: old)
        held = anchor
        frames = frames.filter { $0.key.columnCount == new }
        return anchor
    }

    /// Whether `slot`, laid out at `columnCount`, sits at the top.
    func isOnTop(_ slot: AlbumGridSlot, columnCount: Int) -> Bool {
        frames.contains { placement, shown in
            placement.columnCount == columnCount && shown.slot == slot
                && abs(shown.frame.minY) < 0.5
        }
    }

    private func topSlot(columnCount: Int) -> AlbumGridSlot? {
        frames
            .filter { placement, shown in
                placement.columnCount == columnCount && shown.frame.midY > 0
                    && shown.frame.minY < scroll.visibleHeight
            }
            .map(\.value)
            .min { lhs, rhs in
                (lhs.frame.minY, lhs.frame.minX)
                    < (rhs.frame.minY, rhs.frame.minX)
            }?
            .slot
    }
}

import CoreGraphics

/// A grid slot the scroll can keep on top.
enum AlbumGridSlot: Hashable {
    /// The album slot at a list position.
    case position(Int)
    /// The open album's detail.
    case detail(albumId: String)
}

/// Where the grid's slots sit in the visible area, and the slot a change of
/// column count keeps on top.
///
/// A reference the view holds rather than view state: slots report their
/// frames on every scrolled frame, and nothing the grid draws reads them.
@MainActor
final class AlbumGridViewport {
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
    private var visibleHeight: CGFloat = 0
    /// The slot the last change of column count put on top, held there
    /// until the grid scrolls another way, so changes back and forth return
    /// to the same place.
    private(set) var held: AlbumGridSlot?

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

    func setVisibleHeight(_ height: CGFloat) {
        visibleHeight = height
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
                    && shown.frame.minY < visibleHeight
            }
            .map(\.value)
            .min { lhs, rhs in
                (lhs.frame.minY, lhs.frame.minX)
                    < (rhs.frame.minY, rhs.frame.minX)
            }?
            .slot
    }
}

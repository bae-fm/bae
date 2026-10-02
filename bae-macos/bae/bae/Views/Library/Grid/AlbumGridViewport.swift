import SwiftUI

/// A grid slot the scroll can keep on top.
enum AlbumGridSlot: Hashable {
    /// The album slot at a list position.
    case position(Int)
    /// The open album's detail.
    case detail(albumId: String)
}

/// Where the grid's scroll view stands: its content offset, the height it
/// shows, and the offsets its content scrolls between.
struct AlbumGridScroll: Equatable {
    let offset: CGFloat
    let visibleHeight: CGFloat
    /// The offsets the scroll moves between: from the content's top at the
    /// top of the visible area to its bottom at the bottom.
    let offsets: ClosedRange<CGFloat>
}

/// The parts of the album a pending reveal shows, laid out in one content
/// layout, with that layout's scroll bounds. Everything is in the content's
/// coordinates, so where a part sits does not depend on where the scroll
/// stood when it was measured.
struct RevealLayout: Equatable {
    let seq: Int
    /// The album's card, once the grid knows which position it is at.
    var card: CGRect?
    /// The album's detail, once it shows what it loaded.
    var detail: CGRect?
    /// The row of the track the reveal names.
    var row: CGRect?
    let scroll: AlbumGridScroll

    /// The reveal's next step from this layout; nil while there is nothing
    /// to do but wait for a later layout. `findsCard` once the grid knows
    /// which position the album's card is at.
    ///
    /// A track's row goes in full view: its middle to the middle, or its top
    /// to the top when it is taller than the view. Until the row is laid
    /// out, the album's card goes to the top, which brings its detail in.
    /// An album shows its card on top with its detail under it, as near the
    /// top as the content scrolls; where that is, is known only once the
    /// detail is laid out at its own height. The reveal is shown once a
    /// layout has what it names where it goes, not when a scroll is sent:
    /// the lazy grid can lay out what is above it anew on the way.
    func step(showingTrack: Bool, findsCard: Bool) -> RevealStep? {
        if showingTrack, let row {
            let height = scroll.visibleHeight
            if row.minY > scroll.offset - 0.5,
                row.maxY < scroll.offset + height + 0.5
            {
                return .shown
            }
            let top = row.height > height ? row.minY : row.midY - height / 2
            return .scroll(to: top.clamped(to: scroll.offsets))
        }
        guard let card else { return findsCard ? .findCard : nil }
        let top = card.minY.clamped(to: scroll.offsets)
        guard abs(top - scroll.offset) < 0.5 else { return .scroll(to: top) }
        return !showingTrack && detail != nil ? .shown : nil
    }
}

/// What a reveal does next.
enum RevealStep: Equatable {
    /// What the reveal names is in view where it goes.
    case shown
    /// Scroll to `offset`; the reveal goes on from the layouts that follow.
    case scroll(to: CGFloat)
    /// The album's card is not laid out: scroll to where the grid places it.
    case findCard
}

/// Where the grid's slots sit in the visible area, the slot a change of
/// column count keeps on top, and how far the pending reveal has come.
///
/// A reference the view holds rather than view state: slots report their
/// frames on every scrolled frame, and nothing the grid draws reads them.
@MainActor
final class AlbumGridViewport {
    // periphery:ignore - a dictionary key: hashed and compared, never read.
    /// One shown cell at one column count.
    private struct Placement: Hashable {
        let cell: AlbumGridCell.Identity
        let columnCount: Int
    }

    /// Each shown cell's slot and frame in the visible area, by the column
    /// count it was laid out at, so a layout at a new count never reads as
    /// the one before it.
    private var frames: [Placement: (slot: AlbumGridSlot, frame: CGRect)] =
        [:]
    /// Where the scroll view stands, as it last reported.
    private var scroll: AlbumGridScroll?
    /// The column count the grid last laid a slot out at.
    private(set) var columnCount = 1
    /// The slot the last change of column count put on top, held there
    /// until the grid scrolls another way, so changes back and forth return
    /// to the same place.
    private(set) var held: AlbumGridSlot?
    /// The pending reveal's parts, as the last layout placed them.
    private var revealLayout: RevealLayout?

    func place(
        _ cell: AlbumGridCell.Identity,
        as slot: AlbumGridSlot,
        columnCount: Int,
        frame: CGRect
    ) {
        frames[Placement(cell: cell, columnCount: columnCount)] = (slot, frame)
        self.columnCount = columnCount
    }

    func remove(_ cell: AlbumGridCell.Identity, columnCount: Int) {
        frames.removeValue(
            forKey: Placement(cell: cell, columnCount: columnCount)
        )
    }

    func setScroll(_ scroll: AlbumGridScroll) {
        self.scroll = scroll
    }

    func placeReveal(_ layout: RevealLayout?) {
        revealLayout = layout
    }

    /// The next step of `reveal` from the last layout that placed it,
    /// `findsCard` once the grid knows which position its card is at.
    func step(of reveal: PendingAlbumReveal, findsCard: Bool) -> RevealStep? {
        guard let layout = revealLayout, layout.seq == reveal.seq else {
            return nil
        }
        return layout.step(
            showingTrack: reveal.trackId != nil,
            findsCard: findsCard
        )
    }

    /// The offset that puts the top of row `row`, of `rows` at the current
    /// column count, on top, as the lazy grid places a row it has not laid
    /// out: by the average height of its rows, counted from the top-most
    /// slot laid out in view whose row `rowOf` knows. Nil before the scroll
    /// view has reported where it stands.
    func offset(
        toRow row: Int,
        of rows: Int,
        rowOf: (AlbumGridSlot) -> Int?
    ) -> CGFloat? {
        guard let scroll, rows > 0 else { return nil }
        let content = scroll.offsets.upperBound + scroll.visibleHeight
        let pitch = content / CGFloat(rows)
        let reference =
            frames
            .compactMap { placement, shown -> (row: Int, top: CGFloat)? in
                guard placement.columnCount == columnCount,
                    shown.frame.maxY > 0,
                    shown.frame.minY < scroll.visibleHeight,
                    let row = rowOf(shown.slot)
                else { return nil }
                return (row, scroll.offset + shown.frame.minY)
            }
            .min { $0.top < $1.top }
        let top =
            reference.map { $0.top + CGFloat(row - $0.row) * pitch }
            ?? CGFloat(row) * pitch
        return top.clamped(to: scroll.offsets)
    }

    /// Whether `slot` is laid out in the visible area.
    func shows(_ slot: AlbumGridSlot) -> Bool {
        frames.values.contains { shown in
            shown.slot == slot && shown.frame.maxY > 0
                && shown.frame.minY < (scroll?.visibleHeight ?? 0)
        }
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
                    && shown.frame.minY < (scroll?.visibleHeight ?? 0)
            }
            .map(\.value)
            .min { lhs, rhs in
                (lhs.frame.minY, lhs.frame.minX)
                    < (rhs.frame.minY, rhs.frame.minX)
            }?
            .slot
    }
}

extension CGFloat {
    fileprivate func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}

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
    var offset: CGFloat = 0
    var visibleHeight: CGFloat = 0
    /// The offsets the scroll moves between: from the content's top at the
    /// top of the visible area to its bottom at the bottom.
    var offsets: ClosedRange<CGFloat> = 0...0
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
        return target.clamped(to: scroll.offsets)
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
    private var scroll = AlbumGridScroll()
    /// The slot the last change of column count put on top, held there
    /// until the grid scrolls another way, so changes back and forth return
    /// to the same place.
    private(set) var held: AlbumGridSlot?
    /// The row of the track the pending reveal names, while it is laid out.
    private var revealRow: PlacedTrackRow?
    /// How far the pending reveal's scroll to its album has come, so its
    /// row's place is measured where that scroll stops, not on the way there.
    private var revealScroll: RevealScroll?
    /// What the scroll view is doing: an animated scroll is on its way until
    /// this is idle again.
    private var phase: ScrollPhase = .idle

    private enum RevealScroll {
        /// Scrolling the album's card to the top.
        case toAlbum(seq: Int, card: AlbumGridCell.Identity)
        /// The scroll has come to rest at the album.
        case atAlbum(seq: Int)
    }

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

    /// Reveal `seq` scrolls its album's `card` to the top. Returns `seq`
    /// when the card sits there already, where the scroll moves nothing: the
    /// reveal is at its album.
    func revealScrolls(seq: Int, to card: AlbumGridCell.Identity) -> Int? {
        revealScroll = .toAlbum(seq: seq, card: card)
        return revealArrivedInPlace()
    }

    /// Takes the scroll view's new phase. Returns the reveal whose scroll to
    /// its album this brings to rest: its animated scroll ended, or it came
    /// to rest with the album's card on top.
    func setPhase(from old: ScrollPhase, to new: ScrollPhase) -> Int? {
        phase = new
        guard new == .idle, case .toAlbum(let seq, _) = revealScroll else {
            return nil
        }
        if old == .animating {
            revealScroll = .atAlbum(seq: seq)
            return seq
        }
        return revealArrivedInPlace()
    }

    /// Returns the reveal a scroll that moved without animating, or did not
    /// need to move, has brought to its album: the album's card sits on top
    /// while the scroll view is idle.
    func revealArrivedInPlace() -> Int? {
        guard phase == .idle,
            case .toAlbum(let seq, let card) = revealScroll,
            sitsOnTop(card)
        else { return nil }
        revealScroll = .atAlbum(seq: seq)
        return seq
    }

    /// The row reveal `seq` names, laid out; `atAlbum` asks only once the
    /// reveal has scrolled to its album.
    func revealedRow(seq: Int, atAlbum: Bool = false) -> PlacedTrackRow? {
        guard let revealRow, revealRow.seq == seq else { return nil }
        if atAlbum {
            guard case .atAlbum(seq) = revealScroll else { return nil }
        }
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

    /// Whether `cell` sits where scrolling it to the top puts it: on top, or
    /// as near the top as the content scrolls.
    private func sitsOnTop(_ cell: AlbumGridCell.Identity) -> Bool {
        guard
            let frame = frames.first(where: { $0.key.cell == cell })?.value
                .frame
        else { return false }
        let target = (scroll.offset + frame.minY).clamped(to: scroll.offsets)
        return abs(target - scroll.offset) < 0.5
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

extension CGFloat {
    fileprivate func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}

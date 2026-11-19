import SwiftUI

/// A grid slot the scroll can keep on top.
enum AlbumGridSlot: Hashable {
    /// The album slot at a list position, in the group of the artist
    /// `sectionId` when the grid groups by artist.
    case position(Int, sectionId: String? = nil)
    /// The open album's detail.
    case detail(albumId: String)
}

/// A slot the grid keeps on top, and what keeps it there, which decides how
/// the grid scrolls the slot back when the lazy grid moves it.
struct AlbumGridHold: Equatable {
    enum Reason {
        /// A change of column count. The scroll goes by the slot's cell, so
        /// the scroll position keeps naming the cell and each later change of
        /// width keeps the same slot on top.
        case columnChange
        /// A reveal, scrolling to the album's card. The scroll goes to the
        /// card's top in the content, as the card's latest layout gave it:
        /// the lazy grid moves the card while the scroll position already
        /// names it, and setting a scroll position to the id it names
        /// scrolls nothing. The hold scrolls once for each top the card
        /// takes: a scroll to a point stays in force, and the scroll view
        /// takes the content there as the content grows under it. A second
        /// scroll to the same point can land in the frame where the reveal
        /// arrives and starts its scroll to the track's row, and the scroll
        /// view then writes the point back over the row's scroll. The
        /// reveal's own scroll to the row follows, so nothing needs the
        /// position to name the card afterwards.
        case reveal
    }

    let slot: AlbumGridSlot
    let reason: Reason
}

/// Where a cell is laid out: its frame in the visible area, and its top in
/// the grid's content, where a scroll to an offset measures. One layout gives
/// both.
struct AlbumGridPlacedCell: Equatable {
    let frame: CGRect
    let contentTop: CGFloat
}

/// The held slot to scroll back on top: what holds it, and its top in the
/// content as its latest layout gave it, nil while it is not laid out.
struct AlbumGridHeldSlot: Equatable {
    let hold: AlbumGridHold
    let contentTop: CGFloat?
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
    let frame: CGRect
    let scroll: AlbumGridScroll

    /// The offset showing the whole row, or nil if it is already visible, or
    /// as far into view as the scroll can bring it.
    var scrollOffset: CGFloat? {
        let height = scroll.visibleHeight
        if frame.minY > scroll.offset - 0.5,
            frame.maxY < scroll.offset + height + 0.5
        {
            return nil
        }
        let target =
            frame.height > height ? frame.minY : frame.midY - height / 2
        let clamped = target.clamped(to: scroll.offsets)
        return abs(clamped - scroll.offset) < 0.5 ? nil : clamped
    }
}

/// Where the grid's slots sit in the visible area, the slot a change of
/// column count keeps on top, and how far the grid has come in showing the
/// album the person asked to see.
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

    /// A shown cell: the slot it shows, and where it is laid out.
    private struct Shown {
        let slot: AlbumGridSlot
        let placed: AlbumGridPlacedCell
    }

    /// Each shown cell, by the column count it was laid out at, so a layout
    /// at a new count never reads as the one before it.
    private var frames: [Placement: Shown] = [:]
    private var scroll = AlbumGridScroll()
    /// The slot the last change of column count, or the last reveal, put on
    /// top, held there until the grid scrolls another way, so changes back
    /// and forth return to the same place.
    private var holding: Holding?
    /// The row of the track the reveal names, while it is laid out.
    private var revealRow: PlacedTrackRow?
    /// How far the grid's scroll to the revealed album has come, so the
    /// track's row is measured where that scroll stops, not on the way
    /// there; nil while the grid scrolls for no reveal.
    private var revealScroll: RevealScroll?
    /// What the scroll view is doing: a scroll is on its way until this is
    /// idle again.
    private var phase: ScrollPhase = .idle

    /// A hold, and what the last scroll to put its slot on top was handed
    /// out for, so the same standing asks for no second scroll.
    private struct Holding {
        let hold: AlbumGridHold
        var lastStanding: HoldStanding?
    }

    /// What decides whether the held slot needs another scroll.
    private enum HoldStanding: Equatable {
        /// Where the slot sits and where the scroll stands, for a scroll by
        /// the slot's cell.
        case scroll(HoldScroll)
        /// The revealed card's top in the content, for a scroll to that
        /// point (`AlbumGridHold.Reason.reveal`).
        case contentTop(CGFloat)
    }

    /// Where the held slot sits, and where the scroll stands.
    ///
    /// Periphery, the dead-code check, treats the compiler's generated `==`
    /// as writing these properties without reading them, so a
    /// `periphery:ignore` comment above each property makes Periphery skip
    /// it.
    private struct HoldScroll: Equatable {
        // periphery:ignore
        let slot: AlbumGridSlot
        // periphery:ignore
        let columnCount: Int
        // periphery:ignore
        /// The slot's top in the visible area, nil while it is not drawn.
        let minY: CGFloat?
        // periphery:ignore
        let scroll: AlbumGridScroll
    }

    private enum RevealScroll {
        /// Scrolling the album's card to the top.
        case toAlbum(card: AlbumGridCell.Identity)
        /// The scroll has come to rest at the album.
        case atAlbum
        /// Scrolling the track's row into view.
        case toRow
    }

    func place(
        _ cell: AlbumGridCell.Identity,
        as slot: AlbumGridSlot,
        columnCount: Int,
        placed: AlbumGridPlacedCell
    ) {
        frames[Placement(cell: cell, columnCount: columnCount)] = Shown(
            slot: slot,
            placed: placed
        )
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

    /// The grid scrolls the revealed album's `card` to the top. Returns
    /// whether the card sits there already, where the scroll moves nothing:
    /// the grid is at the album.
    func revealScrolls(to card: AlbumGridCell.Identity) -> Bool {
        revealScroll = .toAlbum(card: card)
        return revealArrivedInPlace()
    }

    /// Takes the scroll view's new phase. Returns whether this brings the
    /// grid to the revealed album: the scroll view comes to rest with the
    /// album's card on top.
    func setPhase(_ new: ScrollPhase) -> Bool {
        phase = new
        return revealArrivedInPlace()
    }

    /// Returns whether the scroll to the revealed album, or a scroll it did
    /// not need, has just brought the grid there: the album's card sits on
    /// top while the scroll view is idle.
    func revealArrivedInPlace() -> Bool {
        guard phase == .idle,
            case .toAlbum(let card) = revealScroll,
            sitsOnTop(card)
        else { return false }
        revealScroll = .atAlbum
        return true
    }

    /// `trackId`'s row, laid out; `arrived` asks only once the grid has
    /// scrolled to the revealed album, from then on through its scroll to
    /// the row.
    func revealedRow(of trackId: String, arrived: Bool = false)
        -> PlacedTrackRow?
    {
        guard let revealRow, revealRow.trackId == trackId else { return nil }
        guard arrived else { return revealRow }
        switch revealScroll {
        case .atAlbum, .toRow:
            return revealRow
        case .toAlbum, nil:
            return nil
        }
    }

    /// The grid scrolls the revealed track's row into view, and measures the
    /// row again wherever it lays it out next.
    func revealScrollsToRow() {
        revealScroll = .toRow
    }

    /// The grid has shown what the reveal names, or starts showing it again:
    /// no scroll of the reveal's is on its way.
    func endRevealScroll() {
        revealScroll = nil
    }

    /// The slot held on top, if any.
    var held: AlbumGridSlot? {
        holding?.hold.slot
    }

    /// Holds the revealed album's `slot` on top until the hold ends.
    func holdForReveal(_ slot: AlbumGridSlot) {
        holding = Holding(hold: AlbumGridHold(slot: slot, reason: .reveal))
    }

    /// Ends the hold, for a scroll or a change in what the grid shows; the
    /// next change of column count keeps whatever is on top then.
    func release() {
        holding = nil
    }

    /// The slot to keep on top as the column count goes from `old` to `new`:
    /// the one held from an earlier change, or the top-most slot at `old`
    /// with at least half of it in view, leftmost in its row. The change
    /// holds the slot from then on, a revealed album's among them. Frames
    /// laid out at other counts are dropped.
    func anchor(from old: Int, to new: Int) -> AlbumGridSlot? {
        let anchor = held ?? topSlot(columnCount: old)
        holding = anchor.map {
            Holding(hold: AlbumGridHold(slot: $0, reason: .columnChange))
        }
        frames = frames.filter { $0.key.columnCount == new }
        return anchor
    }

    /// The hold, when its slot is off the top and has moved, or the scroll
    /// with it, since the last scroll handed out for it; for a reveal's hold
    /// of a laid-out card, when the card's top in the content has moved. A
    /// layout reports the slot several times in the frame a scroll was asked
    /// for, before the scroll takes effect; each report finds the slot where
    /// it was, and a second scroll to the same place would only be asked for
    /// again.
    func scrollBackingHeldSlot(columnCount: Int) -> AlbumGridHeldSlot? {
        guard let hold = holding?.hold else { return nil }
        let slot = hold.slot
        let placed = frames.first {
            $0.key.columnCount == columnCount && $0.value.slot == slot
        }?
        .value.placed
        let standing: HoldStanding =
            if hold.reason == .reveal, let contentTop = placed?.contentTop {
                .contentTop(contentTop)
            }
            else {
                .scroll(
                    HoldScroll(
                        slot: slot,
                        columnCount: columnCount,
                        minY: placed?.frame.minY,
                        scroll: scroll
                    )
                )
            }
        guard !isOnTop(slot, columnCount: columnCount) else {
            // On top, a scroll by cell did its work: the slot drifting off
            // again to where it stood then is a new reason to scroll. A
            // scroll to a point stays in force, so the card at the same top
            // asks for none.
            if case .scroll = standing {
                holding?.lastStanding = nil
            }
            return nil
        }
        guard standing != holding?.lastStanding else { return nil }
        holding?.lastStanding = standing
        return AlbumGridHeldSlot(hold: hold, contentTop: placed?.contentTop)
    }

    /// Whether `slot`, laid out at `columnCount`, sits at the top.
    func isOnTop(_ slot: AlbumGridSlot, columnCount: Int) -> Bool {
        frames.contains { placement, shown in
            placement.columnCount == columnCount && shown.slot == slot
                && abs(shown.placed.frame.minY) < 0.5
        }
    }

    /// Whether `cell` sits where scrolling it to the top puts it: on top, or
    /// as near the top as the content scrolls.
    private func sitsOnTop(_ cell: AlbumGridCell.Identity) -> Bool {
        guard
            let frame = frames.first(where: { $0.key.cell == cell })?.value
                .placed.frame
        else { return false }
        let target = (scroll.offset + frame.minY).clamped(to: scroll.offsets)
        return abs(target - scroll.offset) < 0.5
    }

    private func topSlot(columnCount: Int) -> AlbumGridSlot? {
        frames
            .filter { placement, shown in
                placement.columnCount == columnCount
                    && shown.placed.frame.midY > 0
                    && shown.placed.frame.minY < scroll.visibleHeight
            }
            .map(\.value)
            .min { lhs, rhs in
                (lhs.placed.frame.minY, lhs.placed.frame.minX)
                    < (rhs.placed.frame.minY, rhs.placed.frame.minX)
            }?
            .slot
    }
}

extension CGFloat {
    fileprivate func clamped(to range: ClosedRange<CGFloat>) -> CGFloat {
        Swift.min(Swift.max(self, range.lowerBound), range.upperBound)
    }
}

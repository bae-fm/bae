import BaeKit
import Foundation

/// A lane's geometry in the pane coordinate space, captured off the live
/// layout. The row pitch is not read back from it: the rows region is a lazy
/// stack, so the part of its measured height that is not built yet is
/// SwiftUI's estimate. Rows are sized to `QueueSection.rowHeight` instead, and
/// `rowCount` says whether this lane has any rows to map a cursor onto.
/// `appendFrame` is the lane's trailing zone: the region past the last row
/// that targets an insert-at-end — the whole target when the lane has no rows
/// at all.
private struct QueueLaneGeometry {
    var rowsFrame: CGRect = .null
    var rowCount: Int = 0
    var appendFrame: CGRect = .null
}

/// The in-flight reorder drag. Owned by the row's own `DragGesture` — no
/// AppKit drag session, no floating drag image, no item providers: the lane
/// draws a copy of the row under the cursor, a line marks the gap it will land
/// in while no sibling row moves, and the gesture's `onEnded` is the one deterministic
/// end-of-drag signal (releases, including outside the pane, always deliver
/// it).
private struct ActiveQueueDrag {
    let lane: QueueLaneID
    /// The row that was grabbed. The floating copy draws this value, never a
    /// lookup by index: the canonical index the row started at belongs to
    /// whichever row moves into it the moment core echoes a reorder.
    let item: BridgeQueueEntry
    /// The dragged row's canonical index in its lane at drag start (display
    /// order equals canonical order at that moment — no permutation is live).
    let startSlot: Int
    /// Cursor offset from the row's top at grab time, so the row keeps its
    /// grip point under the cursor instead of snapping its top edge there.
    let grabAnchorY: CGFloat
    /// Cursor position, pane space. Updated continuously by the gesture.
    var location: CGPoint
}

/// A committed reorder kept on screen until the lane's read shows it: the
/// move, which commit it was, and the lane's version core answered the
/// reorder with, once it has.
private struct HeldReorder {
    let move: QueueRowMove
    let commit: Int
    var landsAt: UInt64?
}

/// The released row while it glides from where the cursor let go of it into
/// the slot it lands in. The floating copy keeps drawing it for that stretch
/// (the in-place row stays hidden), so the release reads as the row settling
/// rather than vanishing under the cursor and reappearing at its slot.
private struct SettlingQueueRow {
    let lane: QueueLaneID
    /// The released row, as grabbed — see `ActiveQueueDrag.item`.
    let item: BridgeQueueEntry
    /// The row's canonical index at grab time: what a held reorder's order
    /// names it by.
    let startSlot: Int
    /// The display slot the row settles into: its start slot, or the slot a
    /// held reorder placed it at.
    var landingSlot: Int
}

/// One row of a lane moved from canonical index `from` to display slot `to`,
/// the rest keeping their order around it: the only reorder a drag makes. A
/// lane's display order while a drag or a held reorder is live is this move,
/// read one slot at a time, so it costs the same however many rows the lane
/// has.
struct QueueRowMove: Equatable {
    let from: Int
    let to: Int
    /// The lane's row count the move was made over; a lane of another
    /// length no longer has these slots.
    let count: Int

    /// The canonical index of the row shown at `displaySlot`.
    func source(at displaySlot: Int) -> Int {
        if displaySlot == to {
            return from
        }
        if from < to, (from..<to).contains(displaySlot) {
            return displaySlot + 1
        }
        if to < from, ((to + 1)...from).contains(displaySlot) {
            return displaySlot - 1
        }
        return displaySlot
    }

    /// The display slot the row at canonical index `source` is shown at.
    func displaySlot(of source: Int) -> Int {
        if source == from {
            return to
        }
        if from < to, ((from + 1)...to).contains(source) {
            return source - 1
        }
        if to < from, (to..<from).contains(source) {
            return source + 1
        }
        return source
    }
}

/// What the source lane draws as the floating copy: the grabbed row itself
/// and the top of the copy relative to the lane's rows region.
struct QueueFloatingRow: Equatable {
    let item: BridgeQueueEntry
    let top: CGFloat
}

/// Shared drag state for both `QueueSection`s: the active gesture, the row
/// settling after a release, each lane's measured geometry, and the
/// post-commit hold. All display effects (the drop line, the floating copy's
/// position, the cross-lane insertion gap) are DERIVED from `active` +
/// geometry per render — there is no stored permutation to go stale when the
/// queue changes mid-drag.
@MainActor
@Observable
final class QueueDragCoordinator {
    private var active: ActiveQueueDrag?
    private var settling: SettlingQueueRow?
    private var geometry: [QueueLaneID: QueueLaneGeometry] = [:]
    /// A committed reorder, held per lane so the rows stay put between the
    /// gesture ending and the lane's read showing the move: until core
    /// answers the reorder with the version it made and the read reaches it
    /// (visually a no-op), or core answers that it moved nothing.
    private var held: [QueueLaneID: HeldReorder] = [:]
    /// Each lane's version as its read last showed it.
    private var shownVersion: [QueueLaneID: UInt64] = [:]
    /// How many reorders have been held, which names each commit.
    private var commits = 0
    /// The manual lane's row count, mirrored here by `MacQueuePanel` so the context
    /// section's gesture — which can't see the manual section's props — has it
    /// for cross-lane gap math.
    var manualGapCount = 0

    var isDragging: Bool { active != nil }

    /// Whether `lane` hosts the live drag — its section must also win the
    /// z-order between SECTIONS, or a row dragged across the boundary renders
    /// underneath the sibling section's rows (zIndex only arbitrates among
    /// siblings, and the sections are siblings of each other).
    func isDragSource(_ lane: QueueLaneID) -> Bool {
        active?.lane == lane
    }

    /// Whether the live drag is a context row currently aimed at the manual
    /// lane. While it is, the context lane drops its permutation — its rows
    /// sit in canonical order, since the row is leaving them — and the manual
    /// lane's insertion line marks where the track would land. The dragged row
    /// keeps following the cursor throughout.
    var isCrossLaneTargeting: Bool {
        manualInsertGap(manualCount: manualGapCount) != nil
    }

    /// The entry whose in-place row `lane` hides because the floating copy is
    /// drawing it — the dragged row while a drag is live, the released row
    /// while it settles — else `nil`.
    func floatingEntryId(in lane: QueueLaneID) -> String? {
        if let active, active.lane == lane {
            return active.item.id
        }
        if let settling, settling.lane == lane {
            return settling.item.id
        }
        return nil
    }

    func setRowsGeometry(_ lane: QueueLaneID, frame: CGRect, rowCount: Int) {
        var geo = geometry[lane] ?? QueueLaneGeometry()
        geo.rowsFrame = frame
        geo.rowCount = rowCount
        if geometry[lane]?.rowsFrame != geo.rowsFrame
            || geometry[lane]?.rowCount != geo.rowCount
        {
            geometry[lane] = geo
        }
    }

    func setAppendFrame(_ lane: QueueLaneID, frame: CGRect) {
        var geo = geometry[lane] ?? QueueLaneGeometry()
        geo.appendFrame = frame
        if geometry[lane]?.appendFrame != geo.appendFrame {
            geometry[lane] = geo
        }
    }

    func begin(
        lane: QueueLaneID,
        item: BridgeQueueEntry,
        startSlot: Int,
        location: CGPoint
    ) {
        guard active == nil else {
            return
        }
        // A new grab cuts a still-settling release short: the copy now draws
        // this row, and the settled row's in-place view is shown again.
        settling = nil
        let rowTop: CGFloat
        if let geo = geometry[lane] {
            rowTop =
                geo.rowsFrame.minY + CGFloat(startSlot)
                * QueueSection.rowHeight
        }
        else {
            rowTop = location.y
        }
        active = ActiveQueueDrag(
            lane: lane,
            item: item,
            startSlot: startSlot,
            grabAnchorY: location.y - rowTop,
            location: location
        )
    }

    func update(location: CGPoint) {
        active?.location = location
    }

    /// The slot the dragged row would take in its own lane: the slot under the
    /// cursor, clamped to the lane.
    func gapSlot(count: Int) -> Int? {
        guard let active, let geo = geometry[active.lane],
            geo.rowCount > 0,
            active.startSlot < count
        else {
            return nil
        }
        let pitch = QueueSection.rowHeight
        let rel =
            (active.location.y - active.grabAnchorY + pitch / 2
                - geo.rowsFrame.minY) / pitch
        return min(max(Int(rel.rounded(.down)), 0), count - 1)
    }

    /// The move the lane's display order shows: the held post-commit one while
    /// core's echo is in flight, else none — canonical order. A live drag
    /// moves no row: the lane marks where the row will land with a line
    /// (`dropGap`). Derived against the CURRENT `count`, so a queue change
    /// can never leave a move over slots the lane no longer has.
    func displayMove(for lane: QueueLaneID, count: Int) -> QueueRowMove? {
        if let hold = held[lane], hold.move.count == count {
            return hold.move
        }
        return nil
    }

    /// The move a release of the live same-lane drag makes: the row from its
    /// start slot to the slot under the cursor, or none when it would land
    /// where it is or the drag is aimed at the manual lane.
    private func dragMove(count: Int) -> QueueRowMove? {
        guard let active, !isCrossLaneTargeting,
            let slot = gapSlot(count: count), slot != active.startSlot
        else {
            return nil
        }
        return QueueRowMove(from: active.startSlot, to: slot, count: count)
    }

    /// The between-row gap (0...count) the live same-lane drag will drop its
    /// row into, which the lane draws as a line over the gap; `nil` when the
    /// release would leave the row where it is, or the drag is aimed at the
    /// manual lane. The row after the gap is the lane's canonical row at this
    /// index, whichever way the row moves.
    func dropGap(in lane: QueueLaneID, count: Int) -> Int? {
        guard let active, active.lane == lane,
            let move = dragMove(count: count)
        else {
            return nil
        }
        return move.to > move.from ? move.to + 1 : move.to
    }

    /// The row `lane` draws as a floating copy over its rows region: under
    /// the cursor while a drag is live there, at its landing slot while a
    /// released row settles. `nil` in any lane that has neither.
    func floatingRow(in lane: QueueLaneID) -> QueueFloatingRow? {
        if let active, active.lane == lane, let geo = geometry[lane] {
            return QueueFloatingRow(
                item: active.item,
                top: active.location.y - active.grabAnchorY
                    - geo.rowsFrame.minY
            )
        }
        if let settling, settling.lane == lane {
            return QueueFloatingRow(
                item: settling.item,
                top: CGFloat(settling.landingSlot) * QueueSection.rowHeight
            )
        }
        return nil
    }

    /// Where a context-row drag currently hovers in the MANUAL lane: the
    /// between-row gap (0...count) its track would insert at, or `nil` when
    /// the drag isn't a context row over the manual lane. The manual section
    /// renders its insertion line here; releasing commits the enqueue.
    func manualInsertGap(manualCount: Int) -> Int? {
        guard let active, active.lane == .context,
            let manual = geometry[.manual]
        else {
            return nil
        }
        // Over the rows: the nearest between-row gap. Past them (or when the
        // lane has no rows at all — the whole lane is then just its trailing
        // zone), the append gap at the lane's end.
        if manual.rowCount > 0, active.location.y < manual.rowsFrame.maxY {
            let rel =
                (active.location.y - manual.rowsFrame.minY)
                / QueueSection.rowHeight
            return min(max(Int(rel.rounded()), 0), manualCount)
        }
        if manual.appendFrame.contains(
            CGPoint(x: manual.appendFrame.midX, y: active.location.y)
        ) {
            return manualCount
        }
        return nil
    }

    /// End the drag, resolving what (if anything) to commit. The caller (the
    /// source lane's section, which owns the command callbacks and `itemAt`)
    /// executes the outcome; `reorder` holds the move here until the lane's
    /// next change.
    ///
    /// A same-lane release starts the row settling: the copy glides from the
    /// cursor into the row's start slot — or, once the caller holds a
    /// reorder, the slot that hold places it at — until the caller reports
    /// the release animation `settled()`. A cross-lane release settles
    /// nothing: the track lands in the other lane when the snapshot echoes,
    /// and the source row is back in place at once.
    func finish(count: Int) -> Outcome {
        guard let active else {
            return .none
        }
        defer { self.active = nil }
        if let gap = manualInsertGap(manualCount: manualGapCount) {
            return .insertIntoManual(trackId: active.item.trackId, gap: gap)
        }
        if active.startSlot < count {
            settling = SettlingQueueRow(
                lane: active.lane,
                item: active.item,
                startSlot: active.startSlot,
                landingSlot: active.startSlot
            )
        }
        guard let move = dragMove(count: count) else {
            return .none
        }
        return .reorder(entryId: active.item.id, move: move, lane: active.lane)
    }

    /// Keep a committed reorder on screen until the lane's read shows it,
    /// and answer which commit it is, for core's answer to name. The settling
    /// row lands in the slot the move places it at.
    @discardableResult
    func hold(_ move: QueueRowMove, lane: QueueLaneID) -> Int {
        commits += 1
        held[lane] = HeldReorder(move: move, commit: commits, landsAt: nil)
        if let settling, settling.lane == lane {
            self.settling?.landingSlot = move.displaySlot(
                of: settling.startSlot
            )
        }
        return commits
    }

    /// Core moved the row of `commit`, making the lane's `version`: the hold
    /// ends once the lane's read reaches it, at once when it already has. An
    /// answer for a commit a later one replaced leaves that one held.
    func reorderAnswered(
        _ lane: QueueLaneID,
        commit: Int,
        version: UInt64
    ) {
        guard held[lane]?.commit == commit else {
            return
        }
        if let shown = shownVersion[lane], shown >= version {
            held[lane] = nil
        }
        else {
            held[lane]?.landsAt = version
        }
    }

    /// Core moved nothing for `commit`: the lane shows its read again at
    /// once, so a move that failed never stays on screen.
    func reorderFailed(_ lane: QueueLaneID, commit: Int) {
        guard held[lane]?.commit == commit else {
            return
        }
        held[lane] = nil
    }

    /// The release animation finished: the settled row's in-place view is
    /// shown again and the floating copy is dropped.
    func settled() {
        settling = nil
    }

    /// The lane's read reached `version`. A hold whose reorder made it ends:
    /// the canonical order now equals what was held, so dropping it is
    /// visually a no-op.
    func laneRead(_ lane: QueueLaneID, version: UInt64) {
        shownVersion[lane] = version
        if let landsAt = held[lane]?.landsAt, version >= landsAt {
            held[lane] = nil
        }
    }

    enum Outcome {
        case none
        case reorder(entryId: String, move: QueueRowMove, lane: QueueLaneID)
        case insertIntoManual(trackId: String, gap: Int)
    }
}

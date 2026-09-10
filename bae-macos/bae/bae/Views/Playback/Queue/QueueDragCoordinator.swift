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
/// draws a copy of the row under the cursor, sibling rows shift around its
/// slot continuously, and the gesture's `onEnded` is the one deterministic
/// end-of-drag signal (releases, including outside the pane, always deliver
/// it).
private struct ActiveQueueDrag {
    let lane: QueueLaneID
    /// The row that was grabbed. The floating copy draws this value, never a
    /// lookup by index: the canonical index the row started at belongs to
    /// whichever row moves into it the moment core echoes a reorder.
    let item: QueueItem
    /// The dragged row's canonical index in its lane at drag start (display
    /// order equals canonical order at that moment — no permutation is live).
    let startSlot: Int
    /// Cursor offset from the row's top at grab time, so the row keeps its
    /// grip point under the cursor instead of snapping its top edge there.
    let grabAnchorY: CGFloat
    /// Cursor position, pane space. Updated continuously by the gesture.
    var location: CGPoint
}

/// The released row while it glides from where the cursor let go of it into
/// the slot it lands in. The floating copy keeps drawing it for that stretch
/// (the in-place row stays hidden), so the release reads as the row settling
/// rather than vanishing under the cursor and reappearing at its slot.
private struct SettlingQueueRow {
    let lane: QueueLaneID
    /// The released row, as grabbed — see `ActiveQueueDrag.item`.
    let item: QueueItem
    /// The row's canonical index at grab time: what a held reorder's order
    /// names it by.
    let startSlot: Int
    /// The display slot the row settles into: its start slot, or the slot a
    /// held reorder placed it at.
    var landingSlot: Int
}

/// What the source lane draws as the floating copy: the grabbed row itself
/// and the top of the copy relative to the lane's rows region.
struct QueueFloatingRow: Equatable {
    let item: QueueItem
    let top: CGFloat
}

/// Shared drag state for both `QueueSection`s: the active gesture, the row
/// settling after a release, each lane's measured geometry, and the
/// post-commit hold. All display effects (sibling shifts, the floating copy's
/// position, the cross-lane insertion gap) are DERIVED from `active` +
/// geometry per render — there is no stored permutation to go stale when the
/// queue changes mid-drag.
@MainActor
@Observable
final class QueueDragCoordinator {
    private var active: ActiveQueueDrag?
    private var settling: SettlingQueueRow?
    private var geometry: [QueueLaneID: QueueLaneGeometry] = [:]
    /// A committed reorder's final display order, held per lane so the rows
    /// stay put between the gesture ending and core's snapshot echoing the
    /// same order back (the revision bump clears it — visually a no-op).
    private var hold: [QueueLaneID: [Int]] = [:]
    /// The manual lane's row count, mirrored here by `QueueView` so the context
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
        item: QueueItem,
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

    /// The display slot the dragged row currently occupies in its own lane:
    /// the slot under the cursor, clamped to the lane. Sibling rows shift
    /// around it as this crosses row boundaries — continuously, not on
    /// enter/leave events.
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

    /// The lane's current display order (display slot → canonical index):
    /// the live permutation while a drag is underway here, the held
    /// post-commit order while core's echo is in flight, else canonical.
    /// Derived against the CURRENT `count`, so a queue change mid-drag can
    /// never leave a stale-length permutation — the drag simply falls back
    /// to canonical if its start slot no longer exists.
    func displayOrder(for lane: QueueLaneID, count: Int) -> [Int]? {
        if let active, active.lane == lane {
            guard !isCrossLaneTargeting,
                let gap = gapSlot(count: count), gap != active.startSlot
            else {
                return nil
            }
            var order = Array(0..<count)
            order.remove(at: active.startSlot)
            order.insert(active.startSlot, at: gap)
            return order
        }
        if let held = hold[lane], held.count == count {
            return held
        }
        return nil
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
    /// executes the outcome; `reorder` holds the final order here until the
    /// revision bump.
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
        guard let order = displayOrder(for: active.lane, count: count),
            let gap = gapSlot(count: count)
        else {
            return .none
        }
        return .reorder(
            entryId: active.item.id,
            finalOrder: order,
            gap: gap,
            lane: active.lane
        )
    }

    /// Keep a committed reorder's order on screen until core echoes it back.
    /// The settling row lands in the slot this order places it at.
    func holdOrder(_ order: [Int], lane: QueueLaneID) {
        hold[lane] = order
        if let settling, settling.lane == lane,
            let slot = order.firstIndex(of: settling.startSlot)
        {
            self.settling?.landingSlot = slot
        }
    }

    /// The release animation finished: the settled row's in-place view is
    /// shown again and the floating copy is dropped.
    func settled() {
        settling = nil
    }

    /// Core's snapshot landed for this lane's revision: the canonical order
    /// now equals whatever was held, so dropping the hold is visually a no-op.
    func clearHold(_ lane: QueueLaneID) {
        hold[lane] = nil
    }

    enum Outcome {
        case none
        case reorder(
            entryId: String,
            finalOrder: [Int],
            gap: Int,
            lane: QueueLaneID
        )
        case insertIntoManual(trackId: String, gap: Int)
    }
}

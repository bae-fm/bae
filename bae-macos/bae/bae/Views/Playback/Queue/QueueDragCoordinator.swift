import Foundation

/// A lane's geometry in the pane coordinate space, captured off the live
/// layout. `rowHeight` is the rows-region height over the row count — rows
/// are uniform by design (hover chrome toggles by opacity precisely so they
/// never resize); zero while the lane is empty. `appendFrame` is the lane's
/// trailing zone: the region past the last row that targets an
/// insert-at-end — the whole target when the lane has no rows at all.
private struct QueueLaneGeometry {
    var rowsFrame: CGRect = .null
    var rowHeight: CGFloat = 0
    var appendFrame: CGRect = .null
}

/// The in-flight reorder drag. Owned by the row's own `DragGesture` — no
/// AppKit drag session, no floating drag image, no item providers: the row
/// itself tracks the cursor, sibling rows shift around it continuously, and
/// the gesture's `onEnded` is the one deterministic end-of-drag signal
/// (releases, including outside the pane, always deliver it).
private struct ActiveQueueDrag {
    let lane: QueueLaneID
    let entryId: String
    let trackId: String
    /// The dragged row's canonical index in its lane at drag start (display
    /// order equals canonical order at that moment — no permutation is live).
    let startSlot: Int
    /// Cursor offset from the row's top at grab time, so the row keeps its
    /// grip point under the cursor instead of snapping its top edge there.
    let grabAnchorY: CGFloat
    /// Cursor position, pane space. Updated continuously by the gesture.
    var location: CGPoint
}

/// Shared drag state for both `QueueSection`s: the active gesture, each
/// lane's measured geometry, and the post-commit hold. All display effects
/// (sibling shifts, the dragged row's cursor tracking, the cross-lane
/// insertion gap) are DERIVED from `active` + geometry per render — there is
/// no stored permutation to go stale when the queue changes mid-drag.
@MainActor
@Observable
final class QueueDragCoordinator {
    private var active: ActiveQueueDrag?
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
    /// lane. While it is, the dragged row sits back home (dimmed) and the
    /// manual lane's insertion line is the ONE target indicator — a floating
    /// row and a line drawn through each other read as broken.
    var isCrossLaneTargeting: Bool {
        manualInsertGap(manualCount: manualGapCount) != nil
    }

    /// The dragged entry while a drag is live in `lane`, else `nil`.
    func draggedEntryId(in lane: QueueLaneID) -> String? {
        guard let active, active.lane == lane else {
            return nil
        }
        return active.entryId
    }

    func setRowsGeometry(_ lane: QueueLaneID, frame: CGRect, rowCount: Int) {
        var geo = geometry[lane] ?? QueueLaneGeometry()
        geo.rowsFrame = frame
        geo.rowHeight = rowCount > 0 ? frame.height / CGFloat(rowCount) : 0
        if geometry[lane]?.rowsFrame != geo.rowsFrame
            || geometry[lane]?.rowHeight != geo.rowHeight
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
        entryId: String,
        trackId: String,
        startSlot: Int,
        location: CGPoint
    ) {
        guard active == nil else {
            return
        }
        let rowTop: CGFloat
        if let geo = geometry[lane] {
            rowTop = geo.rowsFrame.minY + CGFloat(startSlot) * geo.rowHeight
        }
        else {
            rowTop = location.y
        }
        active = ActiveQueueDrag(
            lane: lane,
            entryId: entryId,
            trackId: trackId,
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
            geo.rowHeight > 0,
            active.startSlot < count
        else {
            return nil
        }
        let rel =
            (active.location.y - active.grabAnchorY + geo.rowHeight / 2
                - geo.rowsFrame.minY) / geo.rowHeight
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

    /// The dragged row's vertical offset from its current display slot, so it
    /// tracks the cursor exactly while siblings animate around it.
    func draggedRowOffset(displaySlot: Int) -> CGFloat {
        guard let active, let geo = geometry[active.lane] else {
            return 0
        }
        let slotTop =
            geo.rowsFrame.minY + CGFloat(displaySlot) * geo.rowHeight
        return active.location.y - active.grabAnchorY - slotTop
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
        if manual.rowHeight > 0, active.location.y < manual.rowsFrame.maxY {
            let rel =
                (active.location.y - manual.rowsFrame.minY) / manual.rowHeight
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
    func finish(count: Int) -> Outcome {
        guard let active else {
            return .none
        }
        defer { self.active = nil }
        if let gap = manualInsertGap(manualCount: manualGapCount) {
            return .insertIntoManual(trackId: active.trackId, gap: gap)
        }
        guard let order = displayOrder(for: active.lane, count: count),
            let gap = gapSlot(count: count)
        else {
            return .none
        }
        return .reorder(
            entryId: active.entryId,
            finalOrder: order,
            gap: gap,
            lane: active.lane
        )
    }

    /// Keep a committed reorder's order on screen until core echoes it back.
    func holdOrder(_ order: [Int], lane: QueueLaneID) {
        hold[lane] = order
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

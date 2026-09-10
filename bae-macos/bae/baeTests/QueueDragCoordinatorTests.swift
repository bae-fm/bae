import BaeKit
import Foundation
import Testing

@testable import bae

// The whole suite is main-actor isolated: the row pitch it measures against is
// `QueueSection.rowHeight`, a main-actor constant.
@MainActor
@Suite("Queue drag coordinator")
struct QueueDragCoordinatorTests {
    private static let pitch = QueueSection.rowHeight
    private static let items = PreviewData.queueItems

    /// Five manual-lane rows starting at y = 100 in pane space, and five
    /// context-lane rows below them.
    private static let manualRows = CGRect(
        x: 0,
        y: 100,
        width: 420,
        height: 5 * pitch
    )
    private static let contextRows = CGRect(
        x: 0,
        y: manualRows.maxY + 40,
        width: 420,
        height: 5 * pitch
    )

    /// The floating copy is what the lane draws for the released row, so it
    /// has to keep answering after the gesture ends — first at the row's start
    /// slot, then at the slot a held reorder places it at — and stop only when
    /// the release animation reports settled. Otherwise the copy vanishes at
    /// the cursor and the row reappears at its slot, a cut instead of a glide.
    ///
    /// And it answers with the grabbed row itself, not an index: once core
    /// echoes the reorder, the row's old index belongs to whichever row moved
    /// into it, and drawing by index flashed that row for the rest of the
    /// settle.
    @Test("a released row keeps floating at its landing slot until it settles")
    func releasedRowFloatsUntilSettled() throws {
        let coordinator = QueueDragCoordinator()
        coordinator.setRowsGeometry(
            .manual,
            frame: Self.manualRows,
            rowCount: 5
        )
        // Grab row 1 by its middle and carry it two rows down.
        let grabbed = Self.items[1]
        let grab = CGPoint(x: 200, y: Self.manualRows.minY + 1.5 * Self.pitch)
        coordinator.begin(
            lane: .manual,
            item: grabbed,
            startSlot: 1,
            location: grab
        )
        coordinator.update(
            location: CGPoint(x: grab.x, y: grab.y + 2 * Self.pitch)
        )
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 3 * Self.pitch)
        )
        #expect(coordinator.gapSlot(count: 5) == 3)

        let outcome = coordinator.finish(count: 5)
        guard case .reorder(_, let finalOrder, let gap, _) = outcome else {
            Issue.record("expected a same-lane reorder, got \(outcome)")
            return
        }
        #expect(gap == 3)
        #expect(!coordinator.isDragging)
        // Released, not yet held: the in-place row stays hidden and the copy
        // sits at the start slot.
        #expect(coordinator.floatingEntryId(in: .manual) == grabbed.id)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 1 * Self.pitch)
        )

        coordinator.holdOrder(finalOrder, lane: .manual)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 3 * Self.pitch)
        )
        // Core's echo: the canonical order now matches the held one and the
        // hold drops. The copy still draws the grabbed row at its slot.
        coordinator.clearHold(.manual)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 3 * Self.pitch)
        )

        coordinator.settled()
        #expect(coordinator.floatingRow(in: .manual) == nil)
        #expect(coordinator.floatingEntryId(in: .manual) == nil)
    }

    /// A context row released over the manual lane lands there when the
    /// snapshot echoes; nothing settles in the context lane, so its in-place
    /// row shows again at once.
    @Test("a cross-lane release floats nothing")
    func crossLaneReleaseFloatsNothing() {
        let coordinator = QueueDragCoordinator()
        coordinator.manualGapCount = 5
        coordinator.setRowsGeometry(
            .manual,
            frame: Self.manualRows,
            rowCount: 5
        )
        coordinator.setRowsGeometry(
            .context,
            frame: Self.contextRows,
            rowCount: 5
        )
        coordinator.begin(
            lane: .context,
            item: Self.items[0],
            startSlot: 0,
            location: CGPoint(
                x: 200,
                y: Self.contextRows.minY + Self.pitch / 2
            )
        )
        coordinator.update(
            location: CGPoint(
                x: 200,
                y: Self.manualRows.minY + Self.pitch / 2
            )
        )
        #expect(coordinator.isCrossLaneTargeting)

        guard case .insertIntoManual = coordinator.finish(count: 5) else {
            Issue.record("expected a cross-lane enqueue")
            return
        }
        #expect(coordinator.floatingRow(in: .context) == nil)
        #expect(coordinator.floatingEntryId(in: .context) == nil)
    }

    /// Grabbing another row while one is still settling: the copy now draws
    /// the new row, and the settled row's in-place view shows again.
    @Test("a new grab cuts a settling release short")
    func newGrabEndsSettling() {
        let coordinator = QueueDragCoordinator()
        coordinator.setRowsGeometry(
            .manual,
            frame: Self.manualRows,
            rowCount: 5
        )
        coordinator.begin(
            lane: .manual,
            item: Self.items[1],
            startSlot: 1,
            location: CGPoint(
                x: 200,
                y: Self.manualRows.minY + 1.5 * Self.pitch
            )
        )
        _ = coordinator.finish(count: 5)
        #expect(coordinator.floatingEntryId(in: .manual) == Self.items[1].id)

        coordinator.begin(
            lane: .manual,
            item: Self.items[3],
            startSlot: 3,
            location: CGPoint(
                x: 200,
                y: Self.manualRows.minY + 3.5 * Self.pitch
            )
        )
        #expect(coordinator.floatingEntryId(in: .manual) == Self.items[3].id)
        #expect(coordinator.floatingRow(in: .manual)?.item == Self.items[3])
    }
}

import BaeKit
import Foundation
import Testing

@testable import bae

/// The held reorder of the queue drag coordinator: when its hold ends.
extension QueueDragCoordinatorTests {
    static let heldMove = QueueRowMove(from: 1, to: 3, count: 5)

    /// A held reorder ends when the lane's rows draw the moved entry where the
    /// move put it, and not on a change before it: another write to the lane
    /// delivering its rows doesn't put the old order back.
    @Test("a held reorder waits for the lane to draw the move")
    func holdWaitsForTheLaneToDrawTheMove() {
        let coordinator = QueueDragCoordinator()
        coordinator.hold(Self.heldMove, entryId: "moved", lane: .manual)

        #expect(
            !coordinator.holdEnded(.manual, count: 5) { _ in "other" },
            "rows that don't show the move leave it on screen"
        )
        #expect(
            coordinator.displayMove(for: .manual, count: 5) == Self.heldMove
        )
        #expect(
            coordinator.holdEnded(.manual, count: 5) {
                $0 == Self.heldMove.to ? "moved" : "other"
            }
        )
        coordinator.releaseHold(.manual)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
    }

    /// When the lane has a different number of rows than when the person
    /// moved the row, the coordinator ends the hold.
    @Test("a lane of another length ends the hold")
    func laneOfAnotherLengthEndsTheHold() {
        let coordinator = QueueDragCoordinator()
        coordinator.hold(Self.heldMove, entryId: "moved", lane: .context)
        #expect(
            !coordinator.holdEnded(.context, count: 5) { _ in "other" }
        )
        #expect(coordinator.holdEnded(.context, count: 6) { _ in "other" })
        #expect(
            !coordinator.holdEnded(.manual, count: 6) { _ in "other" },
            "a lane holding nothing has nothing to end"
        )
    }

    /// A reorder core refused, or that failed, never stays on screen: the
    /// lane shows its rows at once.
    @Test("a failed reorder shows the lane's rows at once")
    func failedReorderEndsTheHold() {
        let coordinator = QueueDragCoordinator()
        let commit = coordinator.hold(
            Self.heldMove,
            entryId: "moved",
            lane: .manual
        )

        coordinator.reorderFailed(.manual, commit: commit)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
    }

    /// A refused reorder ends the hold with no row arriving, and the settling
    /// copy glides back to where the row still sits, not to the slot the move
    /// would have put it at.
    @Test("a refused reorder ends the hold and sends the copy back")
    func refusedReorderEndsTheHoldAndReturnsTheCopy() throws {
        let coordinator = QueueDragCoordinator()
        coordinator.setRowsGeometry(
            .manual,
            frame: Self.manualRows,
            rowCount: 5
        )
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
        guard case .reorder(_, let move, _) = coordinator.finish(count: 5)
        else {
            Issue.record("expected a same-lane reorder")
            return
        }
        let commit = coordinator.hold(move, entryId: grabbed.id, lane: .manual)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 3 * Self.pitch)
        )

        coordinator.reorderFailed(.manual, commit: commit)
        #expect(
            !coordinator.holdEnded(.manual, count: 5) { _ in "other" },
            "no hold is left to wait for a row"
        )
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 1 * Self.pitch)
        )
    }

    /// The moved row's arrival ends the hold, and a failure reported for that
    /// commit afterwards changes nothing.
    @Test("the moved row's arrival ends the hold")
    func movedRowArrivalEndsTheHold() {
        let coordinator = QueueDragCoordinator()
        let commit = coordinator.hold(
            Self.heldMove,
            entryId: "moved",
            lane: .manual
        )
        #expect(
            coordinator.holdEnded(.manual, count: 5) {
                $0 == Self.heldMove.to ? "moved" : "other"
            }
        )
        coordinator.releaseHold(.manual)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
        coordinator.reorderFailed(.manual, commit: commit)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
    }

    /// Two drops in a row: the first's failure is for a move the second
    /// replaced, so it leaves the second held.
    @Test("an earlier reorder's failure leaves a later hold")
    func earlierFailureLeavesALaterHold() {
        let coordinator = QueueDragCoordinator()
        let first = coordinator.hold(
            Self.heldMove,
            entryId: "moved",
            lane: .manual
        )
        let later = QueueRowMove(from: 0, to: 2, count: 5)
        coordinator.hold(later, entryId: "later", lane: .manual)

        coordinator.reorderFailed(.manual, commit: first)
        #expect(coordinator.displayMove(for: .manual, count: 5) == later)
    }
}

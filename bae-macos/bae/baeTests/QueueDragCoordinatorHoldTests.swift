import BaeKit
import Foundation
import Testing

@testable import bae

/// The held reorder of the queue drag coordinator: when its hold ends.
extension QueueDragCoordinatorTests {
    static let heldMove = QueueRowMove(from: 1, to: 3, count: 5)

    /// A held reorder stays on screen until its write returns, whatever the
    /// lane's rows draw meanwhile, and ends when it returns.
    @Test("a held reorder stays until its write returns")
    func holdStaysUntilTheWriteReturns() {
        let coordinator = QueueDragCoordinator()
        let commit = coordinator.hold(Self.heldMove, lane: .manual)
        #expect(
            coordinator.displayMove(for: .manual, count: 5) == Self.heldMove
        )
        #expect(
            coordinator.displayMove(for: .context, count: 5) == nil,
            "a hold covers only its own lane"
        )

        coordinator.reorderMoved(.manual, commit: commit)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
    }

    /// A reorder core refused, or that failed, never stays on screen: the
    /// lane shows its rows when the write returns.
    @Test("a failed reorder shows the lane's rows when it returns")
    func failedReorderEndsTheHold() {
        let coordinator = QueueDragCoordinator()
        let commit = coordinator.hold(Self.heldMove, lane: .manual)

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
        let commit = coordinator.hold(move, lane: .manual)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 3 * Self.pitch)
        )

        coordinator.reorderFailed(.manual, commit: commit)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)
        #expect(
            coordinator.floatingRow(in: .manual)
                == QueueFloatingRow(item: grabbed, top: 1 * Self.pitch)
        )
    }

    /// Two drops in a row: when the first reorder's write returns, whether
    /// the move succeeded or failed, the coordinator keeps holding the second
    /// move.
    @Test("an earlier reorder's return leaves a later hold")
    func earlierReturnLeavesALaterHold() {
        let coordinator = QueueDragCoordinator()
        let first = coordinator.hold(Self.heldMove, lane: .manual)
        let later = QueueRowMove(from: 0, to: 2, count: 5)
        coordinator.hold(later, lane: .manual)

        coordinator.reorderMoved(.manual, commit: first)
        #expect(coordinator.displayMove(for: .manual, count: 5) == later)
        coordinator.reorderFailed(.manual, commit: first)
        #expect(coordinator.displayMove(for: .manual, count: 5) == later)
    }
}

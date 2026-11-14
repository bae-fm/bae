import BaeKit
import Foundation
import Testing

@testable import bae

/// While a row is held over a lane, a line marks the gap it will land in and no
/// row moves: the gap is the one after the slot the row takes when it moves
/// down, the one before it when it moves up, and none while it would land where
/// it started. The row after the gap is the lane's own row at that index, which
/// a release names for core.
@MainActor
@Suite("Queue drop line")
struct QueueDropLineTests {
    private static let pitch = QueueSection.rowHeight
    private static let rows = CGRect(
        x: 0,
        y: 100,
        width: 420,
        height: 5 * pitch
    )

    @Test("a held row's drop is the gap it will land in, and no row moves")
    func dropGapMarksWhereTheRowLands() {
        let coordinator = QueueDragCoordinator()
        coordinator.setRowsGeometry(.manual, frame: Self.rows, rowCount: 5)
        let grab = CGPoint(x: 200, y: Self.rows.minY + 1.5 * Self.pitch)
        coordinator.begin(
            lane: .manual,
            item: PreviewData.queueEntries[1],
            startSlot: 1,
            location: grab
        )
        #expect(coordinator.dropGap(in: .manual, count: 5) == nil)

        coordinator.update(
            location: CGPoint(x: grab.x, y: grab.y + 2 * Self.pitch)
        )
        #expect(coordinator.dropGap(in: .manual, count: 5) == 4)
        #expect(coordinator.dropGap(in: .context, count: 5) == nil)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)

        coordinator.update(
            location: CGPoint(x: grab.x, y: grab.y - 2 * Self.pitch)
        )
        #expect(coordinator.dropGap(in: .manual, count: 5) == 0)
        #expect(coordinator.displayMove(for: .manual, count: 5) == nil)

        coordinator.update(
            location: CGPoint(x: grab.x, y: grab.y + 3 * Self.pitch)
        )
        #expect(coordinator.dropGap(in: .manual, count: 5) == 5)
        guard case .reorder(_, let move, _) = coordinator.finish(count: 5)
        else {
            Issue.record("expected a same-lane reorder")
            return
        }
        #expect(move == QueueRowMove(from: 1, to: 4, count: 5))
        #expect(coordinator.dropGap(in: .manual, count: 5) == nil)
    }
}

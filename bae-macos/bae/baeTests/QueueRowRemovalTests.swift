import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Queue row removal")
struct QueueRowRemovalTests {
    /// A removed row collapses first, and its remove goes to core when the
    /// collapse animation reports it has finished, so core's echo can't cut
    /// the collapse short.
    @MainActor
    @Test("a removed row is sent to core when its collapse finishes")
    func removeIsSentWhenTheCollapseFinishes() async throws {
        let removals = RemovalRecorder()
        let items = Array(PreviewData.queueEntries.prefix(3))
        try await SnapshotTestSupport.withHostedWindow(
            QueueSection(
                title: nil,
                shuffled: false,
                count: items.count,
                itemAt: { index in
                    items.indices.contains(index) ? items[index] : nil
                },
                holdsEntry: { id in items.contains { $0.id == id } },
                loadVersion: 0,
                loadRange: nil,
                acceptsExternalDrops: false,
                laneId: .manual,
                coordinator: QueueDragCoordinator(),
                laneVersion: 1,
                onClear: {},
                onSkipTo: { _ in },
                onRemove: { id in removals.record(id) },
                onReorder: { _, _ in 1 },
                onReorderFailed: { _ in },
                onInsertTracks: { _, _ in },
                onSetShuffle: nil,
            )
            .frame(maxHeight: .infinity, alignment: .top)
            .coordinateSpace(name: "queuePane")
            .environment(ImageStore.stub()),
            size: NSSize(width: 420, height: 400)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // A row near the bottom of the lane, which sits at the top of the
            // host: a row and a half up from the lane's end clears the spacer
            // under the last row.
            let fromTop =
                host.fittingSize.height - 1.5 * QueueSection.rowHeight
            let lastRow = host.convert(
                NSPoint(
                    x: host.bounds.midX,
                    y: host.isFlipped
                        ? fromTop : host.bounds.height - fromTop
                ),
                to: nil
            )
            let menu = try #require(
                try HostedInput.contextMenu(at: lastRow, in: host)
            )
            let remove = try #require(
                menu.items.firstIndex {
                    $0.title == String(localized: "Remove from queue")
                },
                "the row's menu: \(menu.items.map(\.title))"
            )
            menu.performActionForItem(at: remove)

            try await Wait.until { !removals.ids.isEmpty }
            #expect(removals.ids.count == 1)
            #expect(items.map(\.id).contains(removals.ids[0]))
        }
    }

    /// The entries the hosted section asked core to remove.
    @MainActor
    private final class RemovalRecorder {
        private(set) var ids: [String] = []

        func record(_ id: String) {
            ids.append(id)
            Wait.happened()
        }
    }
}

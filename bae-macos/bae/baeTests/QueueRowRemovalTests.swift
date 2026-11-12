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
            // Right-click the row: its menu opens in a panel of its own, and
            // a line is picked as VoiceOver picks it.
            try openContextMenu(at: lastRow, in: host)
            let element = try #require(
                menuItem(titled: String(localized: "Remove from queue")),
                "the row's menu has Remove from queue"
            )
            #expect(element.accessibilityPerformPress())

            try await Wait.until { !removals.ids.isEmpty }
            #expect(removals.ids.count == 1)
            #expect(items.map(\.id).contains(removals.ids[0]))
        }
    }

    /// A right click at `point` (in the window's coordinates) on the view
    /// that takes right clicks there.
    @MainActor
    private func openContextMenu(at point: NSPoint, in host: NSView) throws {
        let window = try #require(host.window)
        let catcher = try #require(
            SnapshotTestSupport.descendants(of: host)
                .last {
                    String(describing: type(of: $0))
                        .contains("ContextClickView")
                        && $0.convert($0.bounds, to: nil).contains(point)
                },
            "a view takes the row's right click"
        )
        let event = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown,
                location: point,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: 0,
                clickCount: 1,
                pressure: 1
            )
        )
        catcher.rightMouseDown(with: event)
    }

    /// The open menu's line called `title`, as VoiceOver finds it.
    @MainActor
    private func menuItem(titled title: String) -> NSAccessibilityElement? {
        for window in NSApp.windows {
            guard let content = window.contentView,
                content.accessibilityRole() == .menu
            else { continue }
            let items = content.accessibilityChildren() as? [Any] ?? []
            return items.compactMap { $0 as? NSAccessibilityElement }
                .first { $0.accessibilityLabel() == title }
        }
        return nil
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

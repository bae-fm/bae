import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Queue section row virtualization")
struct QueueSectionVirtualizationTests {
    /// The context lane counts the whole rest of the library, so opening the
    /// pane must not mount — and must not fetch for — rows thousands of slots
    /// below the viewport. The store keeps only three page subscriptions at a
    /// time, so a whole-lane mount also evicts the pages the visible rows are
    /// waiting on and leaves them placeholders.
    @MainActor
    @Test("only rows near the viewport ask the store to load")
    func loadsOnlyNearTheViewport() async throws {
        let recorder = LoadRecorder()
        let size = NSSize(width: 420, height: 565)
        let hosted = SnapshotTestSupport.hostInWindow(
            ScrollView {
                QueueSection(
                    title: nil,
                    shuffled: false,
                    count: 5_000,
                    itemAt: { _ in nil },
                    loadEpoch: 1,
                    loadRange: { offset, limit in
                        await recorder.record(offset: offset, limit: limit)
                    },
                    acceptsExternalDrops: false,
                    laneId: .context,
                    coordinator: QueueDragCoordinator(),
                    queueRevision: 1,
                    onClear: {},
                    onSkipTo: { _ in },
                    onRemove: { _ in },
                    onReorder: { _, _ in },
                    onInsertTracks: { _, _ in },
                    onSetShuffle: nil,
                )
            }
            .coordinateSpace(name: "queuePane")
            .environment(ImageStore.stub()),
            size: size
        )
        hosted.window.isReleasedWhenClosed = false
        defer { hosted.window.close() }
        await SnapshotTestSupport.settle(hosted.host)

        // 565pt of 62pt rows is about nine visible rows, each fetching the
        // 100-row page around itself, and the lazy stack prepares a modest
        // buffer past the viewport. 600 leaves room for that buffer while
        // still failing a mount of the whole lane.
        #expect(!recorder.requests.isEmpty)
        #expect(recorder.requests.allSatisfy { $0.offset + $0.limit <= 600 })
    }

    /// Every range the hosted section asked the store for.
    @MainActor
    private final class LoadRecorder {
        private(set) var requests: [(offset: Int, limit: Int)] = []

        func record(offset: Int, limit: Int) {
            requests.append((offset: offset, limit: limit))
        }
    }
}

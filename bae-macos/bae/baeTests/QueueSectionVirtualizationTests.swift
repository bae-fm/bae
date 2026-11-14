import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Queue section row virtualization")
struct QueueSectionVirtualizationTests {
    /// The context lane counts the whole rest of the library, so opening the
    /// pane must not mount — and must not hold places for — rows thousands of
    /// slots below the viewport: each place held is a window the list reads.
    @MainActor
    @Test("only rows near the viewport hold their place in the list")
    func holdsOnlyNearTheViewport() async throws {
        let recorder = HoldRecorder()
        let size = NSSize(width: 420, height: 565)
        try await SnapshotTestSupport.withHostedWindow(
            ScrollView {
                QueueSection(
                    title: nil,
                    shuffled: false,
                    count: 5_000,
                    itemAt: { _ in nil },
                    loadEpoch: nil,
                    holdRow: { index in
                        recorder.record(index)
                    },
                    acceptsExternalDrops: false,
                    laneId: .context,
                    coordinator: QueueDragCoordinator(),
                    onClear: {},
                    onSkipTo: { _ in },
                    onRemove: { _ in },
                    onReorder: { _, _ in true },
                    onReorderFailed: { _ in },
                    onInsertTracks: { _, _ in },
                    onSetShuffle: nil,
                )
            }
            .coordinateSpace(name: "queuePane")
            .environment(ImageStore.stub()),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)

            // 565pt of 62pt rows is about nine visible rows, and the lazy stack
            // prepares a modest buffer past the viewport. 600 leaves room for
            // that buffer while still failing a mount of the whole lane.
            #expect(!recorder.holds.isEmpty)
            #expect(recorder.holds.allSatisfy { $0 < 600 })
        }
    }

    /// Every place the hosted section held in the list.
    @MainActor
    private final class HoldRecorder {
        private(set) var holds: [Int] = []

        func record(_ index: Int) {
            holds.append(index)
        }
    }
}

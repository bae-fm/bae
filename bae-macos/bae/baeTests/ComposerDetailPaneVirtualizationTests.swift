import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Composer detail pane row virtualization")
struct ComposerDetailPaneVirtualizationTests {
    /// Core caps neither a composer's work count nor its credits, so opening a
    /// prolific composer must not mount — and must not start a cover load for
    /// — the work rows hundreds of slots below the viewport.
    @MainActor
    @Test("only rows near the viewport ask for their cover")
    func loadsOnlyNearTheViewport() async throws {
        let recorder = CoverRequestRecorder()
        let imageStore = ImageStore(
            fetchLibraryImageBytes: { ref in
                await recorder.record(ref.id)
                return nil
            }
        )
        let uiStore = UiStore()
        let backing = LibraryView.previewComposerBacking(
            uiStore: uiStore,
            libraryStore: PreviewData.seededComposerStore()
        )
        let detail = PreviewData.largeComposerDetail(workCount: 5_000)
        let size = NSSize(width: 620, height: 720)
        let hosted = SnapshotTestSupport.hostInWindow(
            ComposerDetailPane(paneDetail: .composer(detail, work: nil))
                .environment(backing.session)
                .environment(uiStore)
                .environment(imageStore),
            size: size
        )
        hosted.window.isReleasedWhenClosed = false
        defer { hosted.window.close() }
        await SnapshotTestSupport.settle(hosted.host)
        // The mounted rows start their cover loads in tasks of their own; give
        // those a turn to reach the store before counting what it was asked
        // for.
        try await Task.sleep(for: .milliseconds(250))

        // 720pt of ~56pt rows is about thirteen visible rows, and the lazy
        // stack prepares a modest buffer past the viewport. 100 leaves room
        // for that buffer while still failing a mount of the whole works list.
        #expect(!recorder.requested.isEmpty)
        #expect(recorder.requested.count < 100)
    }

    /// Every library image the hosted pane's rows asked the store to load.
    @MainActor
    private final class CoverRequestRecorder {
        private(set) var requested: Set<String> = []

        func record(_ imageId: String) {
            requested.insert(imageId)
        }
    }
}

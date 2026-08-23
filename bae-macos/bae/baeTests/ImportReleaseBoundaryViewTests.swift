import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import release boundary rendering")
struct ImportReleaseBoundaryViewTests {
    @MainActor
    @Test("clicking an ambiguity title cannot hide its evidence")
    func ambiguityCannotHideItsEvidence() async throws {
        let size = NSSize(width: 360, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            FolderReleaseBoundaryRow(
                boundary: PreviewData.folderReleaseBoundary,
                onDecision: { _, _ in }
            )
            .frame(
                maxWidth: .infinity,
                maxHeight: .infinity,
                alignment: .topLeading
            ),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let before = try await SnapshotTestSupport.capturePNG(host, size: size)

        let point = NSPoint(x: 24, y: size.height - 18)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: point,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            window.sendEvent(event)
        }
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let after = try await SnapshotTestSupport.capturePNG(host, size: size)
        #expect(before == after)
    }

    @MainActor
    @Test(
        "choosing an ambiguity decision clears the selected release immediately"
    )
    func decisionClearsSelectionBeforeCoreCompletes() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setFolderCandidateSelection([
            "candidate:selected-before-decision"
        ])
        let importer = Importer(
            setFolderReleaseDecision: { _, _ in
                try await Task.sleep(for: .seconds(30))
            }
        )
        // The boundary card is the tab's only row, so the click below lands on
        // its decision. Which rows a tab holds is core's answer now, so that is
        // stated as a narrower fixture rather than as a filter the view applies.
        let scene = PreviewData.releaseQueueScene()
        let slot = ImportListSlot.preview(
            importStore: scene.store,
            uiStore: uiStore,
            items: [PreviewData.releaseQueueBoundaryItem]
        )
        let size = NSSize(width: 1200, height: 760)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportView()
                .environment(uiStore)
                .importPreviewEnvironment()
                .environment(uiStore)
                .environment(slot)
                .environment(Library.stub())
                .environment(PreviewAudio.stub())
                .environment(scene.store)
                .environment(importer),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let point = NSPoint(x: 105, y: size.height - 238)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: point,
                    modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber,
                    context: nil,
                    eventNumber: 0,
                    clickCount: 1,
                    pressure: type == .leftMouseDown ? 1 : 0
                )
            )
            window.sendEvent(event)
        }
        #expect(uiStore.selectedFolderCandidates.isEmpty)
    }
}

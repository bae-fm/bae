import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Import candidate checkbox")
struct ImportCandidateCheckboxTests {
    @MainActor
    @Test("an unchecked candidate can be checked inside the selectable list")
    func uncheckedCandidateCanBeChecked() async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        uiStore.setImportCandidateFilterText(
            PreviewData.triageRowReady.folderName
        )
        let listSelection = CandidateListSelection()
        let size = NSSize(width: 400, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateListContent(
                importStore: PreviewData.importTabStore(),
                selectedKeys: listSelection.binding,
                onAddFolder: {},
                onRemoveFolder: { _ in },
                onRefreshFolder: { _ in },
                onReleaseDecision: { _, _ in },
                onSkip: { _, _ in },
                onImportSelected: { _ in }
            )
            .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
            .environment(uiStore)
            .environment(ImageStore.stub())
            .frame(width: size.width, height: size.height),
            size: size
        )
        host.layoutSubtreeIfNeeded()
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        let checkbox = try #require(
            descendants(of: host).compactMap { $0 as? NSButton }
                .first {
                    $0.frame.width <= 24 && $0.frame.height <= 24
                }
        )
        let checkboxCenter = checkbox.convert(
            NSPoint(x: checkbox.bounds.midX, y: checkbox.bounds.midY),
            to: nil
        )
        click(checkboxCenter, in: window)
        try await Task.sleep(for: .milliseconds(50))

        #expect(
            uiStore.selectedReadyCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
    }

    @MainActor
    private func click(_ point: NSPoint, in window: NSWindow) {
        let timestamp = ProcessInfo.processInfo.systemUptime
        let mouseDown = NSEvent.mouseEvent(
            with: .leftMouseDown,
            location: point,
            modifierFlags: [],
            timestamp: timestamp,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            clickCount: 1,
            pressure: 1
        )
        let mouseUp = NSEvent.mouseEvent(
            with: .leftMouseUp,
            location: point,
            modifierFlags: [],
            timestamp: timestamp + 0.01,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 0,
            clickCount: 1,
            pressure: 0
        )
        if let mouseDown, let mouseUp {
            NSApp.postEvent(mouseUp, atStart: false)
            window.sendEvent(mouseDown)
        }
    }

    @MainActor
    private func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }
}

@MainActor
private final class CandidateListSelection {
    private var keys: Set<String> = []

    var binding: Binding<Set<String>> {
        Binding(
            get: { self.keys },
            set: { self.keys = $0 }
        )
    }
}

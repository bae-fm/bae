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

        let buttons: [NSButton] = descendants(of: host)
            .compactMap {
                $0 as? NSButton
            }
        let checkbox = try #require(buttons.first)
        let checkboxCenter = checkbox.convert(
            NSPoint(x: checkbox.bounds.midX, y: checkbox.bounds.midY),
            to: nil
        )
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = try #require(
                NSEvent.mouseEvent(
                    with: type,
                    location: checkboxCenter,
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

        #expect(
            uiStore.selectedReadyCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
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

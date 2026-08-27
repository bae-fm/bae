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
        try await assertCheckboxCanBeChecked(isGroupMember: false)
    }

    @MainActor
    @Test("a group member checkbox stays inside the selectable list")
    func groupedCandidateCanBeChecked() async throws {
        try await assertCheckboxCanBeChecked(isGroupMember: true)
    }

    @MainActor
    private func assertCheckboxCanBeChecked(
        isGroupMember: Bool
    ) async throws {
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        // One Ready row, so the first checkbox in the rendered list is the one
        // this test clicks. Which rows a tab holds is core's answer now, so a
        // narrower list is stated as a narrower fixture rather than as a
        // filter the view applies.
        let store = PreviewData.importTabScene().store
        let slot = ImportListSlot.preview(
            importStore: store,
            uiStore: uiStore,
            items: [
                .candidate(
                    stableKey:
                        "candidate:\(PreviewData.triageRowReady.candidateKey)",
                    row: PreviewData.triageRowReady,
                    isGroupMember: isGroupMember
                )
            ]
        )
        let listSelection = CandidateListSelection()
        let size = NSSize(width: 400, height: 320)
        let (window, host) = SnapshotTestSupport.hostInWindow(
            ImportCandidateListContent(
                importStore: store,
                listSlot: slot,
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

        let buttons: [NSButton] = SnapshotTestSupport.descendants(of: host)
            .compactMap {
                $0 as? NSButton
            }
        let checkbox = try #require(buttons.first)
        try click(checkbox, in: window)
        await Task.yield()

        #expect(
            uiStore.selectedReadyCandidates
                == [PreviewData.triageRowReady.candidateKey]
        )
    }

    @MainActor
    private func click(_ checkbox: NSButton, in window: NSWindow) throws {
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

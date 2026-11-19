import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct MacImportTabsTests {
    @Test(
        "Import tabs accept clicks across their full segment",
        arguments: [320.0, 600.0, 900.0]
    )
    func fullHitArea(width: Double) async throws {
        let selection = Selection()
        let size = NSSize(width: width, height: 40)
        let listSlot = ImportListSlot.preview(
            importStore: PreviewData.importTabScene().store,
            uiStore: UiStore(),
            items: [],
            tabCounts: BridgeTriageTabCounts(
                pending: 170,
                done: 32,
                skipped: 19
            )
        )
        try await SnapshotTestSupport.withHostedWindow(
            MacImportTabs(
                activeTab: Binding(
                    get: { selection.tab },
                    set: { selection.tab = $0 }
                ),
                listSlot: listSlot
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let tabs: [BridgeTriageTab] = [.pending, .done, .skipped]
            let segmentWidth = (width - 8) / 3
            for (index, tab) in tabs.enumerated() {
                // Exercise blank padding on every side of the label and badge.
                for point in [
                    NSPoint(x: 4, y: 20),
                    NSPoint(x: segmentWidth - 4, y: 20),
                    NSPoint(x: segmentWidth / 2, y: 9),
                    NSPoint(x: segmentWidth / 2, y: 31),
                ] {
                    selection.tab = tab == .pending ? .done : .pending
                    try await SnapshotTestSupport.settle(host)
                    try HostedInput.click(
                        at: NSPoint(
                            x: Double(index) * (segmentWidth + 4) + point.x,
                            y: point.y
                        ),
                        in: window
                    )
                    try await SnapshotTestSupport.settle(host)
                    #expect(selection.tab == tab)
                }
            }
        }
    }

    @Test("each tab is named with its count once the count arrives")
    func tabNames() {
        #expect(MacImportTabs.name(of: .pending, count: 14) == "Found, 14")
        #expect(MacImportTabs.name(of: .done, count: 31) == "Imported, 31")
        #expect(MacImportTabs.name(of: .skipped, count: nil) == "Skipped")
    }

    @Observable
    fileprivate final class Selection {
        var tab: BridgeTriageTab = .pending
    }
}

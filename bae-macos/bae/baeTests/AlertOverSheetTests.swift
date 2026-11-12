import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An alert is never behind a sheet: raised while one is open, it sits over
/// that sheet, and nothing below it can be reached.
@MainActor
@Suite("Alerts over sheets")
struct AlertOverSheetTests {
    /// A window with the shared error alert below it.
    private struct Window: View {
        let uiStore: UiStore

        var body: some View {
            Color.clear.errorAlert(uiStore)
        }
    }

    /// A sheet holding a `MacSheet`, as the app's sheets are.
    private func macSheet() -> NSPanel {
        let sheet = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 120),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        sheet.isReleasedWhenClosed = false
        sheet.contentView = NSHostingView(
            rootView: MacSheet(
                "Sheet title",
                width: 300,
                content: { Text(verbatim: "Content") },
                footer: { EmptyView() }
            )
        )
        return sheet
    }

    @Test(
        "the shared error alert shows over an open sheet and is the frontmost window"
    )
    func theErrorAlertShowsOverTheSheet() async throws {
        let uiStore = UiStore()
        try await SnapshotTestSupport.withHostedWindow(
            Window(uiStore: uiStore),
            size: NSSize(width: 600, height: 400)
        ) { window, host in
            let sheet = macSheet()
            window.beginSheet(sheet, completionHandler: nil)
            SnapshotTestSupport.takeSheetNotices()
            #expect(window.attachedSheet === sheet)

            uiStore.showError(
                DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "dns error"
                )
            )
            try await SnapshotTestSupport.settle(host)
            await spin(until: { sheet.attachedSheet != nil })
            SnapshotTestSupport.takeSheetNotices()

            let alert = try #require(sheet.attachedSheet)
            #expect(alert.isVisible)
            #expect(alert.canBecomeKey)
            #expect(alert.attachedSheet == nil)
            // The alert is the top of the chain: it sits over the sheet
            // that sits over the window, so the window and the sheet are the
            // ones blocked.
            #expect(alert.sheetParent === sheet)
            #expect(sheet.sheetParent === window)

            // It is a key-capable panel of its own, the last of the alerts
            // showing, and sized to its words and buttons.
            #expect(AlertWindows.shared.panels.last === alert)
            #expect(alert.frame.width > 0 && alert.frame.height > 0)

            uiStore.clearError()
            await spin(until: { sheet.attachedSheet == nil })
            SnapshotTestSupport.takeSheetNotices()
            #expect(sheet.attachedSheet == nil)
            #expect(AlertWindows.shared.panels.isEmpty)
            window.endSheet(sheet)
            sheet.orderOut(nil)
            sheet.contentView = nil
            SnapshotTestSupport.takeSheetNotices()
        }
    }

    /// Lets the main run loop turn, where SwiftUI applies a change, until
    /// `condition` holds; a condition that never holds is for the caller's
    /// expectation to report.
    private func spin(until condition: @MainActor () -> Bool) async {
        for _ in 0..<100 where !condition() {
            try? await Task.sleep(for: .milliseconds(50))
        }
    }
}

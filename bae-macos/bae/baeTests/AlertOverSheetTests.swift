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
        ) { window, _ in
            let sheet = macSheet()
            autoreleasepool { window.beginSheet(sheet, completionHandler: nil) }
            try await SnapshotTestSupport.untilSheet(on: window, attached: true)
            #expect(autoreleasepool { window.attachedSheet } === sheet)

            uiStore.showError(
                DisplayError(
                    line: "Could not reach the cloud.",
                    detail: "dns error"
                )
            )
            try await SnapshotTestSupport.untilSheet(on: sheet, attached: true)

            let alert = try #require(autoreleasepool { sheet.attachedSheet })
            #expect(alert.isVisible)
            #expect(alert.canBecomeKey)
            #expect(autoreleasepool { alert.attachedSheet } == nil)
            // The alert is the top of the chain: it sits over the sheet
            // that sits over the window, so the window and the sheet are the
            // ones blocked.
            #expect(autoreleasepool { alert.sheetParent } === sheet)
            #expect(autoreleasepool { sheet.sheetParent } === window)

            // It is a key-capable panel of its own, the last of the alerts
            // showing, and sized to its words and buttons.
            #expect(AlertWindows.shared.panels.last === alert)
            #expect(alert.frame.width > 0 && alert.frame.height > 0)

            uiStore.clearError()
            try await SnapshotTestSupport.untilSheet(on: sheet, attached: false)
            #expect(AlertWindows.shared.panels.isEmpty)
            autoreleasepool {
                window.endSheet(sheet)
                sheet.orderOut(nil)
                sheet.contentView = nil
            }
            try await SnapshotTestSupport.untilSheet(
                on: window,
                attached: false
            )
        }
    }
}

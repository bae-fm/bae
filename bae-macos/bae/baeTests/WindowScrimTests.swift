import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A sheet or an alert dims the whole window behind it, title bar included:
/// one scrim window across it for as long as it shows.
@MainActor
@Suite("Window scrim")
struct WindowScrimTests {
    /// The scrim windows over `window`.
    private func scrims(over window: NSWindow) -> [NSWindow] {
        autoreleasepool {
            (window.childWindows ?? [])
                .filter { $0 is ScrimWindow }
        }
    }

    private func holds(
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () -> Bool
    ) async -> Bool {
        do {
            try await SnapshotTestSupport.untilWindows(
                file: file,
                line: line,
                condition
            )
            return true
        }
        catch { return false }
    }

    @Test("an alert dims the window it sits over until it closes")
    func anAlertDimsItsWindow() async throws {
        let uiStore = UiStore()
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear.errorAlert(uiStore),
            size: NSSize(width: 600, height: 400)
        ) { window, _ in
            #expect(scrims(over: window).isEmpty)

            uiStore.showError(
                DisplayError(line: "Could not reach the cloud.", detail: nil)
            )
            #expect(
                await holds { scrims(over: window).count == 1 },
                "the alert dims the window"
            )
            let scrim = try #require(scrims(over: window).first)
            #expect(scrim.frame == window.frame)

            uiStore.clearError()
            #expect(
                await holds { scrims(over: window).isEmpty },
                "the scrim goes with the alert"
            )
        }
    }

    @Test("a sheet dims the window it hangs from until it ends")
    func aSheetDimsItsWindow() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear,
            size: NSSize(width: 600, height: 400)
        ) { window, _ in
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
            #expect(scrims(over: window).isEmpty)

            autoreleasepool { window.beginSheet(sheet, completionHandler: nil) }
            try await SnapshotTestSupport.untilSheet(on: window, attached: true)
            #expect(
                await holds { scrims(over: window).count == 1 },
                "the sheet dims the window"
            )
            // The sheet stays in front of the scrim.
            let scrim = try #require(scrims(over: window).first)
            #expect(scrim.frame == window.frame)
            let order = autoreleasepool { NSApp.orderedWindows }
            let sheetIndex = try #require(order.firstIndex(of: sheet))
            let scrimIndex = try #require(order.firstIndex(of: scrim))
            #expect(
                sheetIndex < scrimIndex,
                "the sheet is in front of the scrim"
            )

            autoreleasepool {
                window.endSheet(sheet)
                sheet.orderOut(nil)
                sheet.contentView = nil
            }
            try await SnapshotTestSupport.untilSheet(
                on: window,
                attached: false
            )
            #expect(
                await holds { scrims(over: window).isEmpty },
                "the scrim goes with the sheet"
            )
        }
    }
}

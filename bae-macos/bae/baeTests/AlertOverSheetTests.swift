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

    /// Whether `condition` holds within the deadline, read at each idle of
    /// the run loop: what it reads, AppKit's windows, publishes no change.
    private func holds(
        _ condition: @MainActor () -> Bool
    ) async -> Bool {
        let end = ContinuousClock.now.advanced(by: Wait.deadline)
        // The window server's notices of a window made key or ordered in are
        // handled in a pool of their own, as for a sheet, so they do not hold
        // the hosted window past its test.
        while !condition() {
            SnapshotTestSupport.takeSheetNotices()
            if condition() { return true }
            do {
                try await Wait.nextIdle(
                    before: end,
                    timeout: Wait.deadline,
                    file: #filePath,
                    line: #line
                )
            }
            catch { return condition() }
        }
        return true
    }

    /// What stops the sheet below the alert from being reached: by pointer,
    /// keyboard and VoiceOver.
    private func blocksTheSheet(
        _ sheet: NSWindow,
        alert: NSPanel,
        shield: NSWindow
    ) async throws {
        // The pointer: a window that takes clicks covers the sheet, above
        // it and below the alert, so every click on the sheet lands on it.
        #expect(shield.frame == sheet.frame)
        #expect(!shield.ignoresMouseEvents)
        #expect(shield.level == sheet.level)
        let (below, above) = try autoreleasepool {
            let children = try #require(sheet.childWindows)
            return (
                try #require(children.firstIndex(of: shield)),
                try #require(children.firstIndex(of: alert))
            )
        }
        #expect(below < above)

        // The keyboard: making the sheet key hands the key back to the
        // alert.
        autoreleasepool { NSApp.activate(ignoringOtherApps: true) }
        #expect(await holds { NSApp.isActive }, "the app is active")
        #expect(
            await holds { alert.isKeyWindow },
            "the raised alert is key"
        )
        autoreleasepool { sheet.makeKey() }
        #expect(
            await holds { alert.isKeyWindow },
            "the key came back to the alert"
        )

        // VoiceOver: the alert is the one modal element.
        #expect(autoreleasepool { alert.isAccessibilityModal() })
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
            #expect(
                await holds { !AlertWindows.shared.panels.isEmpty },
                "the alert is raised"
            )

            let alert = try #require(AlertWindows.shared.panels.last)
            let shield = try #require(AlertWindows.shared.shields.last)
            #expect(alert.isVisible)
            #expect(alert.canBecomeKey)
            // The alert is the top of the chain: it sits over the sheet
            // that sits over the window, so the window and the sheet are the
            // ones blocked.
            #expect(AlertWindows.shared.parent(of: alert) === sheet)
            #expect(autoreleasepool { sheet.sheetParent } === window)
            #expect(
                autoreleasepool { sheet.childWindows?.contains(alert) } == true
            )
            #expect(
                autoreleasepool { sheet.childWindows?.contains(shield) } == true
            )

            try await blocksTheSheet(sheet, alert: alert, shield: shield)

            // It is a key-capable panel of its own, the last of the alerts
            // showing, and sized to its words and buttons.
            #expect(alert.frame.width > 0 && alert.frame.height > 0)
            // The panel's top is the sheet's top, at its centre; its face hangs 36 points below.
            #expect(alert.frame.maxY == sheet.frame.maxY)
            #expect(abs(alert.frame.midX - sheet.frame.midX) < 1)

            uiStore.clearError()
            #expect(
                await holds { AlertWindows.shared.panels.isEmpty },
                "the alert is withdrawn"
            )
            #expect(AlertWindows.shared.shields.isEmpty)
            #expect(autoreleasepool { sheet.childWindows?.isEmpty } ?? true)
            // The keyboard returns to the sheet the alert was over.
            #expect(
                await holds { sheet.isKeyWindow },
                "the key returned to the sheet"
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
        }
    }

    @Test(
        "an alert asked for as its window's view is first built is raised once the view has a window"
    )
    func anAlertAskedForAtFirstBuildIsRaised() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear.macAlert(isPresented: .constant(true)) {
                (
                    MacAlertWords(Text(verbatim: "Title")),
                    [MacAlertButton(Text(verbatim: "OK"))]
                )
            },
            size: NSSize(width: 600, height: 400)
        ) { window, _ in
            #expect(
                await holds { AlertWindows.shared.panels.count == 1 },
                "the alert is raised"
            )
            let alert = try #require(AlertWindows.shared.panels.first)
            #expect(AlertWindows.shared.parent(of: alert) === window)
        }
    }
}

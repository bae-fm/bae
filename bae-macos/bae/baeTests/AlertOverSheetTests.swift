import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

/// An alert is never behind a sheet: raised while one is open, it sits over
/// that sheet, and nothing below it can be reached.
@MainActor
@Suite("Alerts over sheets", .serialized)
struct AlertOverSheetTests {
    /// The stack that every window of the test presents on.
    let modals = ModalWindows()

    /// A window that can host the shared error alert.
    private struct Window: View {
        let uiStore: UiStore
        let kind: ErrorAlertWindow
        let modals: ModalWindows

        var body: some View {
            Color.clear.errorAlert(uiStore, in: kind).modalWindows(modals)
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

    /// Whether `condition` holds within the hang guard, read as the windows
    /// change (`SnapshotTestSupport.untilWindows`).
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

    /// What stops the sheet below the alert from being reached: by pointer,
    /// keyboard and VoiceOver.
    private func blocksTheSheet(
        _ sheet: NSWindow,
        alert: NSPanel,
        shield: NSWindow
    ) async throws {
        // The pointer: a window that takes clicks covers the sheet, above
        // it and below the alert, so the covering window receives every click
        // aimed at the sheet.
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
            Window(uiStore: uiStore, kind: .main, modals: modals),
            size: NSSize(width: 600, height: 400)
        ) { window, _ in
            window.identifier = ErrorAlertWindow.main.identifier
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
                await holds { !modals.panels.isEmpty },
                "the alert is raised"
            )

            let alert = try #require(modals.panels.last)
            let shield = try #require(modals.shields.last)
            #expect(alert.isVisible)
            #expect(alert.canBecomeKey)
            // The alert is the top of the chain: it sits over the sheet
            // that sits over the window, so the window and the sheet are the
            // ones blocked.
            #expect(modals.parent(of: alert) === sheet)
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
            // The panel's top is the sheet's top, at its centre; the panel
            // draws the alert 36 points below its top.
            #expect(alert.frame.maxY == sheet.frame.maxY)
            #expect(abs(alert.frame.midX - sheet.frame.midX) < 1)

            uiStore.clearError()
            #expect(
                await holds { modals.panels.isEmpty },
                "the alert is withdrawn"
            )
            #expect(modals.shields.isEmpty)
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

    /// A view that applies both forms of `macAlert` to its own state, which
    /// `asked` and `named` set, and tells `ran` of each run of its body.
    private struct Caller: View {
        let asked: PassthroughSubject<Bool, Never>
        let named: PassthroughSubject<String?, Never>
        let ran: () -> Void

        @State
        private var isAsking = false
        @State
        private var name: String?

        var body: some View {
            ran()
            return Color.clear
                .onReceive(asked) { isAsking = $0 }
                .onReceive(named) { name = $0 }
                .macAlert(isPresented: $isAsking) { alert("Asked") }
                .macAlert(item: $name) { alert($0) }
        }

        private func alert(_ title: String) -> (
            MacAlertWords, [MacAlertButton]
        ) {
            (
                MacAlertWords(Text(verbatim: title)),
                [MacAlertButton(Text(verbatim: "OK"))]
            )
        }
    }

    /// A view that presents an asker's questions and tells `ran` of each run
    /// of its body.
    private struct AskerCaller: View {
        let asker: AlertAsker
        let ran: () -> Void

        var body: some View {
            ran()
            return Color.clear.presentsAlerts(of: asker)
        }
    }

    @Test("raising a macAlert doesn't run the body of the view it is on")
    func theCallerDoesNotReadTheBinding() async throws {
        let asked = PassthroughSubject<Bool, Never>()
        let named = PassthroughSubject<String?, Never>()
        var runs = 0
        try await SnapshotTestSupport.withHostedWindow(
            Caller(asked: asked, named: named) { runs += 1 }
                .modalWindows(modals),
            size: NSSize(width: 600, height: 400)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let first = runs

            asked.send(true)
            #expect(
                await holds { modals.panels.count == 1 },
                "the alert is raised"
            )
            asked.send(false)
            #expect(
                await holds { modals.panels.isEmpty },
                "the alert is withdrawn"
            )
            named.send("Named")
            #expect(
                await holds { modals.panels.count == 1 },
                "the item's alert is raised"
            )
            named.send(nil)
            #expect(
                await holds { modals.panels.isEmpty },
                "the item's alert is withdrawn"
            )

            #expect(runs == first)
        }
    }

    @Test(
        "macAlert shows an alert that a view asks for before it has a window, once the view gets one"
    )
    func anAlertAskedForAtFirstBuildIsRaised() async throws {
        try await SnapshotTestSupport.withHostedWindow(
            Color.clear
                .macAlert(isPresented: .constant(true)) {
                    (
                        MacAlertWords(Text(verbatim: "Title")),
                        [MacAlertButton(Text(verbatim: "OK"))]
                    )
                }
                .modalWindows(modals),
            size: NSSize(width: 600, height: 400)
        ) { window, _ in
            #expect(
                await holds { modals.panels.count == 1 },
                "the alert is raised"
            )
            let alert = try #require(modals.panels.first)
            #expect(modals.parent(of: alert) === window)
        }
    }
}

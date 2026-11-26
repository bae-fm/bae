import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// An alert dims the whole window behind it, title bar included: one scrim
/// window across it for as long as it shows. A system sheet adds none, since
/// macOS dims the window it hangs from.
@MainActor
@Suite("Scrim window")
struct ScrimWindowTests {
    /// The stack the test's window presents its alert on.
    let modals = ModalWindows()

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
        try await SnapshotTestSupport.withHostedWindow(
            AnyView(Color.clear),
            size: NSSize(width: 600, height: 400)
        ) { window, host in
            #expect(scrims(over: window).isEmpty)

            // A view that asks for its alert as it is first built raises it.
            host.rootView = AnyView(
                Color.clear
                    .macAlert(isPresented: .constant(true)) {
                        (
                            MacAlertWords(Text(verbatim: "Title")),
                            [MacAlertButton(Text(verbatim: "OK"))]
                        )
                    }
                    .modalWindows(modals)
            )
            #expect(
                await holds { scrims(over: window).count == 1 },
                "the alert dims the window"
            )
            let scrim = try #require(scrims(over: window).first)
            #expect(scrim.frame == window.frame)

            host.rootView = AnyView(Color.clear)
            #expect(
                await holds { scrims(over: window).isEmpty },
                "the scrim goes with the alert"
            )
        }
    }

    @Test(
        "a system sheet adds no dimming of its own over the window it hangs from"
    )
    func aSheetAddsNoScrim() async throws {
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

            autoreleasepool { window.beginSheet(sheet, completionHandler: nil) }
            try await SnapshotTestSupport.untilSheet(on: window, attached: true)
            // macOS dims the window behind a system sheet by itself; a scrim
            // of the app's would dim it twice. A sheet that put one up did so
            // by the time it was attached and its first update had run.
            try await Wait.nextIdle(
                before: ContinuousClock.now.advanced(by: .seconds(10)),
                file: #filePath,
                line: #line
            )
            #expect(
                scrims(over: window).isEmpty,
                "the app dims the window behind a system sheet"
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
}

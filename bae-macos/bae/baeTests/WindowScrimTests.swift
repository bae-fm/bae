import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A sheet or an alert dims the whole window behind it, title bar included:
/// one scrim across the window's frame view for as long as it shows.
@MainActor
@Suite("Window scrim")
struct WindowScrimTests {
    /// The scrims across `window`'s frame view, which holds the title bar and
    /// the content.
    private func scrims(over window: NSWindow) -> [NSView] {
        autoreleasepool {
            (window.contentView?.superview?.subviews ?? [])
                .filter {
                    $0.accessibilityIdentifier()
                        == WindowScrim.accessibilityIdentifier
                }
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
            let frameView = try #require(window.contentView?.superview)
            #expect(scrim.frame == frameView.bounds)

            uiStore.clearError()
            #expect(
                await holds { scrims(over: window).isEmpty },
                "the scrim goes with the alert"
            )
        }
    }
}

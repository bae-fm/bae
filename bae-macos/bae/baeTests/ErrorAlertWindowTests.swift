import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The shared error alert shows in one window.
@MainActor
@Suite("Error alert window choice", .serialized)
struct ErrorAlertWindowTests {
    /// A window that can host the shared error alert.
    private struct Window: View {
        let uiStore: UiStore
        let kind: ErrorAlertWindow

        var body: some View {
            Color.clear.errorAlert(uiStore, in: kind)
        }
    }

    @Test("a shared error shows only in the front window")
    func aSharedErrorShowsOnlyInTheFrontWindow() async throws {
        let uiStore = UiStore()
        let size = NSSize(width: 600, height: 400)
        try await SnapshotTestSupport.withHostedWindow(
            Window(uiStore: uiStore, kind: .settings),
            size: size
        ) { back, _ in
            back.identifier = ErrorAlertWindow.settings.identifier
            try await SnapshotTestSupport.withHostedWindow(
                Window(uiStore: uiStore, kind: .main),
                size: size
            ) { front, _ in
                front.identifier = ErrorAlertWindow.main.identifier
                uiStore.showError("Could not reach the cloud.")
                defer { uiStore.clearError() }
                try await SnapshotTestSupport.untilWindows {
                    AlertWindows.shared.panels.contains {
                        AlertWindows.shared.parent(of: $0) === front
                    }
                }
                try await Wait.nextIdle(
                    before: ContinuousClock.now.advanced(
                        by: HangGuard.duration
                    ),
                    file: #filePath,
                    line: #line
                )
                #expect(
                    autoreleasepool { AlertWindows.shared.panels.count } == 1
                )
                #expect(autoreleasepool { back.childWindows?.isEmpty } ?? true)

                uiStore.showError(
                    DisplayError(line: "Could not save the window position."),
                    in: .settings
                )
                try await SnapshotTestSupport.untilWindows {
                    AlertWindows.shared.panels.contains {
                        AlertWindows.shared.parent(of: $0) === back
                    }
                }
                #expect(AlertWindows.shared.panels.count == 1)
                #expect(autoreleasepool { front.childWindows?.isEmpty } ?? true)
                uiStore.clearError()
                try await SnapshotTestSupport.untilWindows {
                    AlertWindows.shared.panels.isEmpty
                }
            }
        }
    }
}

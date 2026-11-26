import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The shared error alert shows in one window.
@MainActor
@Suite("Error alert window choice", .serialized)
struct ErrorAlertWindowTests {
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

    @Test("a shared error shows only in the front window")
    func aSharedErrorShowsOnlyInTheFrontWindow() async throws {
        let uiStore = UiStore()
        let size = NSSize(width: 600, height: 400)
        try await SnapshotTestSupport.withHostedWindow(
            Window(uiStore: uiStore, kind: .settings, modals: modals),
            size: size
        ) { back, _ in
            back.identifier = ErrorAlertWindow.settings.identifier
            try await SnapshotTestSupport.withHostedWindow(
                Window(uiStore: uiStore, kind: .main, modals: modals),
                size: size
            ) { front, _ in
                front.identifier = ErrorAlertWindow.main.identifier
                uiStore.showError("Could not reach the cloud.")
                defer { uiStore.clearError() }
                try await SnapshotTestSupport.untilWindows {
                    modals.panels.contains {
                        modals.parent(of: $0) === front
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
                    autoreleasepool { modals.panels.count } == 1
                )
                #expect(autoreleasepool { back.childWindows?.isEmpty } ?? true)

                uiStore.showError(
                    DisplayError(line: "Could not save the window position."),
                    in: .settings
                )
                try await SnapshotTestSupport.untilWindows {
                    modals.panels.contains {
                        modals.parent(of: $0) === back
                    }
                }
                #expect(modals.panels.count == 1)
                #expect(autoreleasepool { front.childWindows?.isEmpty } ?? true)
                uiStore.clearError()
                try await SnapshotTestSupport.untilWindows {
                    modals.panels.isEmpty
                }
            }
        }
    }
}

import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("UndoneSetupLine")

/// A storage setup that ended undone with no setup screen there to hear it,
/// such as one bae was stopped partway through: why, and Dismiss.
struct UndoneSetupLine: View {
    @Environment(SyncStatusStore.self)
    private var status
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    /// The Dismiss running; nil while none runs.
    @State
    private var dismissTask: Task<Void, Never>?

    var body: some View {
        if let undone = status.snapshot?.undoneSetup,
            let error = DisplayError(undone.error)
        {
            MacNotice(
                error: error,
                tone: .warning,
                action: ErrorAction("Dismiss", isDisabled: dismissTask != nil) {
                    dismiss(undone.operation)
                }
            )
            .aboveBox()
        }
    }

    private func dismiss(_ operation: Int64) {
        dismissTask = Task {
            defer { dismissTask = nil }
            do { try await sync.dismissUndoneStorageSetup(operation) }
            catch {
                logger.error(
                    "Dismissing the undone setup failed: \(String(reflecting: error))"
                )
                uiStore.showError(error)
            }
        }
    }
}

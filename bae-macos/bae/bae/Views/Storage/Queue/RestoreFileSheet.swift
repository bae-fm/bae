import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RestoreFileSheet")

/// A source file that moved before it uploaded: put it back, then retry.
/// `heading` says which file and what happened to it; `detail` is the error
/// behind it, when one is kept.
struct RestoreFileSheet: View {
    let heading: String
    let detail: String?
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            Text(heading)
                .themeText(.heading)
            Text(
                "Put the file back where it was when it was imported, then retry."
            )
            .themeText(.body)
            .foregroundStyle(Theme.macTextSecondary)
            if let detail {
                Text(detail)
                    .themeText(.mono)
                    .textSelection(.enabled)
            }
            if let error {
                MacNotice(error: error)
            }
            MacSheetFooter {
                Button("Cancel", action: onDismiss)
                    .keyboardShortcut(.cancelAction)
                MacButton("Retry", kind: .primary) { retry() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(task != nil)
            }
        }
        .padding()
        .frame(width: 440)
        .onDisappear { task?.cancel() }
    }

    private func retry() {
        error = nil
        task = Task {
            defer { task = nil }
            do {
                try await sync.retryOutbox()
                onDismiss()
            }
            catch is CancellationError {
                logger.debug("upload retry cancelled")
            }
            catch {
                self.error = DisplayError(error)
            }
        }
    }
}

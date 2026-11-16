import BaeKit
import SwiftUI

/// Saving the cover picked, shared by the cover pickers: whether a save runs,
/// and why the last one failed.
@MainActor
@Observable
final class CoverPickerState {
    private(set) var isSaving = false
    private(set) var error: DisplayError?
    private var saveTask: Task<Void, Never>?

    func save(
        _ apply: @escaping () async throws -> Void,
        onSaved: @escaping () -> Void
    ) {
        guard !isSaving else { return }
        isSaving = true
        error = nil
        saveTask = Task {
            defer { isSaving = false }
            do {
                try await apply()
                try Task.checkCancellation()
                onSaved()
            }
            catch is CancellationError {}
            catch {
                self.error = DisplayError(error)?
                    .rewording {
                        String(localized: "Could not change the cover: \($0)")
                    }
            }
        }
    }

    func cancel() {
        saveTask?.cancel()
    }
}

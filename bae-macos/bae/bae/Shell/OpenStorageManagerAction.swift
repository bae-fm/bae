import SwiftUI
import os.log

private let openStorageManagerLogger = Logger.bae("OpenStorageManagerAction")

/// Opens the Storage Manager window, or brings it forward when it is open;
/// the application installs it in the primary window's views.
struct OpenStorageManagerAction {
    let open: @MainActor () -> Void

    @MainActor
    func callAsFunction() {
        open()
    }
}

extension EnvironmentValues {
    /// A test hosts views without the application's windows.
    @Entry
    var openStorageManager = OpenStorageManagerAction {
        openStorageManagerLogger.warning(
            "The Storage Manager asked for in a host without its window"
        )
    }
}

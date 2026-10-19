import SwiftUI
import os.log

private let openSettingsLogger = Logger.bae("OpenSettingsWindowAction")

/// Opens the settings window, or brings it forward when it is open; the
/// application installs it in the primary window's views.
struct OpenSettingsWindowAction {
    let open: @MainActor () -> Void

    @MainActor
    func callAsFunction() {
        open()
    }
}

extension EnvironmentValues {
    /// A test hosts views without the application's windows.
    @Entry
    var openSettingsWindow = OpenSettingsWindowAction {
        openSettingsLogger.warning(
            "Settings asked for in a host without a settings window"
        )
    }
}

import AppKit
import SwiftUI

/// The app's windows: the primary one, Settings and the Storage Manager.
/// Each is built when it opens and let go when it closes.
extension AppDelegate {
    /// Show the primary window, building it if it is closed.
    func showMainWindow() {
        let controller =
            mainWindow
            ?? MainWindowController(
                appDelegate: self,
                library: { [unowned self] in
                    self.hasShell ? self.appService?.mainAppMenuTarget : nil
                },
                onClose: { [weak self] in self?.mainWindow = nil }
            )
        mainWindow = controller
        controller.showWindow(nil)
    }

    @objc
    func showSettingsWindow(_: Any?) {
        let controller =
            settingsWindow
            ?? HostingWindowController(
                content: SettingsWindowRoot(
                    appDelegate: self,
                    checkForUpdatesViewModel: requiredApplicationServices
                        .checkForUpdatesViewModel
                )
                .appAppearance(),
                // Fixed at what the content asks for.
                contentSize: nil,
                sizing: [.minSize, .maxSize],
                // The settings' sidebar sits in the window's toolbar.
                bridging: .toolbars,
                onClose: { [weak self] in self?.settingsWindow = nil }
            )
        controller.window?.title = String(localized: "Settings")
        settingsWindow = controller
        controller.showWindow(nil)
    }

    @objc
    func showStorageManager(_: Any?) {
        let controller =
            storageManagerWindow
            ?? HostingWindowController(
                content: StorageManagerWindowRoot(appDelegate: self)
                    .appAppearance(),
                contentSize: CGSize(width: 800, height: 500),
                sizing: .minSize,
                bridging: .toolbars,
                onClose: { [weak self] in self?.storageManagerWindow = nil }
            )
        controller.window?.title = String(localized: "Storage Manager")
        storageManagerWindow = controller
        controller.showWindow(nil)
    }
}

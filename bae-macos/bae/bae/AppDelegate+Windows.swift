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
                    self.appService?.mainAppMenuTarget
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
                // The window has no toolbar: its panes sit in the content's
                // own sidebar.
                bridging: [],
                onClose: { [weak self] in self?.settingsWindow = nil }
            )
        if let window = controller.window {
            // The content reaches under the title bar, which shows only the
            // window buttons, over the sidebar. The title is for the Window
            // menu and VoiceOver.
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.title = String(localized: "Settings")
        }
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
                contentSize: CGSize(width: 1100, height: 660),
                sizing: .minSize,
                bridging: .toolbars,
                onClose: { [weak self] in self?.storageManagerWindow = nil }
            )
        controller.window?.title = String(localized: "Storage manager")
        storageManagerWindow = controller
        controller.showWindow(nil)
    }
}

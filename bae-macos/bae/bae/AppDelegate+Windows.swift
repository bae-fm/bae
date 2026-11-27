import AppKit
import BaeKit
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
                .appAppearance()
                .modalWindows(modalWindows),
                // Fixed at what the content asks for.
                contentSize: nil,
                sizing: [.minSize, .maxSize],
                // The window has no toolbar: its panes sit in the content's
                // own sidebar.
                bridging: [],
                framePlacement: framePlacement(
                    named: "Settings",
                    in: .settings
                ),
                configureWindow: { window in
                    // The title bar shows only the window buttons, over the
                    // sidebar. The title is for the Window menu and VoiceOver.
                    window.showOnlyWindowButtons()
                    window.title = String(localized: "Settings")
                    window.identifier = ErrorAlertWindow.settings.identifier
                },
                onClose: { [weak self] in self?.settingsWindow = nil }
            )
        settingsWindow = controller
        controller.showWindow(nil)
    }

    @objc
    func showStorageManager(_: Any?) {
        let controller =
            storageManagerWindow
            ?? HostingWindowController(
                content: StorageManagerWindowRoot(appDelegate: self)
                    .themedWindowTitleBar()
                    .appAppearance()
                    .modalWindows(modalWindows),
                contentSize: CGSize(width: 1100, height: 660),
                sizing: .minSize,
                bridging: .toolbars,
                framePlacement: framePlacement(
                    named: "Storage Manager",
                    in: .storageManager
                ),
                configureWindow: {
                    $0.title = String(localized: "Storage manager")
                    $0.identifier = ErrorAlertWindow.storageManager.identifier
                },
                onClose: { [weak self] in self?.storageManagerWindow = nil }
            )
        storageManagerWindow = controller
        controller.showWindow(nil)
    }

    private func framePlacement(
        named name: String,
        in window: ErrorAlertWindow
    )
        -> WindowFrameAutosave.Opening
    {
        WindowFrameAutosave.Opening(
            autosave: .init(
                name: name,
                defaults: .standard,
                bugReporter: bugReports,
                showError: { [weak self] error in
                    // Placement happens before AppKit shows the new window.
                    Task { @MainActor in
                        self?.uiStore.showError(error, in: window)
                    }
                }
            ),
            preferredScreen: mainWindow?.window?.screen,
            screens: NSScreen.screens
        )
    }
}

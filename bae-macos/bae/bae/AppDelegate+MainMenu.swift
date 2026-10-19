import AppKit
import BaeKit

/// The main menu's commands that act on the app rather than on the library
/// the primary window shows. The delegate is at the end of every responder
/// chain, so these are enabled whichever window is key.
extension AppDelegate: NSMenuItemValidation {
    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        switch menuItem.action {
        case #selector(checkForUpdates(_:)):
            // An inert host has no updater.
            guard let applicationServices else { return false }
            return applicationServices.checkForUpdatesViewModel
                .canCheckForUpdates
        case #selector(restorePlaybackOnLaunch(_:)):
            menuItem.state = restoresPlaybackOnLaunch ? .on : .off
            return true
        default:
            return true
        }
    }

    @objc
    func checkForUpdates(_: Any?) {
        requiredApplicationServices.checkForUpdatesViewModel.checkForUpdates()
    }

    @objc
    func newLibrary(_: Any?) {
        presentWelcome(mode: nil)
    }

    @objc
    func joinLibrary(_: Any?) {
        presentWelcome(mode: .join)
    }

    @objc
    func restoreFromCode(_: Any?) {
        presentWelcome(mode: .restore(nil))
    }

    @objc
    func openListedLibrary(_ sender: NSMenuItem) {
        guard let libraryId = sender.representedObject as? String else {
            preconditionFailure("an Open Library item holds its library's id")
        }
        openLocalLibrary(id: libraryId)
    }

    /// Sets the opposite of what the item showed when its menu opened, in
    /// the device-local default the Playback settings pane writes.
    @objc
    func restorePlaybackOnLaunch(_ sender: NSMenuItem) {
        UserDefaults.standard.set(
            sender.state != .on,
            forKey: "persistPlayback"
        )
    }

    /// The "Restore on launch" preference.
    private var restoresPlaybackOnLaunch: Bool {
        UserDefaults.standard.bool(forKey: "persistPlayback")
    }
}

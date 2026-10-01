import Foundation

extension AppDelegate {
    /// Everything the main menu shows, read from where each value lives.
    func readMenuBar(_ services: ApplicationServices) -> MenuBarState {
        MenuBarState(
            canCheckForUpdates:
                services.checkForUpdatesViewModel.canCheckForUpdates,
            libraries: libraries.map(LibraryMenuEntry.init),
            clipboard: services.firstResponderActions.performable(),
            // The "Restore on launch" preference, the device-local default
            // the Playback settings pane writes.
            restoresPlaybackOnLaunch: UserDefaults.standard.bool(
                forKey: "persistPlayback"
            ),
            library: appService.map {
                LibraryMenuState(reading: $0.mainAppMenuTarget)
            }
        )
    }
}

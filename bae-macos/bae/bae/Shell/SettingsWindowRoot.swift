import SwiftUI

/// The Settings window's content. A `View` so Observation tracks its
/// `appService` read and it re-renders once the library opens.
struct SettingsWindowRoot: View {
    let appDelegate: AppDelegate
    let checkForUpdatesViewModel: CheckForUpdatesViewModel

    var body: some View {
        if let appService = appDelegate.appService {
            appService.installEnvironment(
                MacSettingsWindow(
                    checkForUpdatesViewModel: checkForUpdatesViewModel,
                    onResetDevice: {
                        appDelegate.presentDeviceReset(
                            libraryId: appService.libraryId
                        )
                    },
                    onRestoreFromCode: {
                        appDelegate.presentDeviceRestore(
                            libraryId: appService.libraryId
                        )
                    },
                    onRenameLibrary: { appDelegate.presentRenameLibrary() },
                    onRemoveLibrary: { appDelegate.closeLibrary() }
                )
                .errorAlert(appDelegate.uiStore)
                .onAppear { appService.reportScreen(.settings) }
            )
        }
        else {
            MacContentUnavailable(
                title: "No library loaded",
                description: "Open a library first to access settings"
            )
            .frame(
                width: NoLibraryPlaceholder.width,
                height: NoLibraryPlaceholder.height
            )
        }
    }
}

/// A window's "No library loaded" placeholder.
enum NoLibraryPlaceholder {
    static let width: CGFloat = 300
    static let height: CGFloat = 200
}

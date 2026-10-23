import SwiftUI

/// The Settings window's content. A `View` so Observation tracks its
/// `appService` read and it re-renders once the library opens.
struct SettingsWindowRoot: View {
    let appDelegate: AppDelegate
    let checkForUpdatesViewModel: CheckForUpdatesViewModel

    var body: some View {
        if let appService = appDelegate.appService {
            appService.installEnvironment(
                SettingsView(
                    checkForUpdatesViewModel: checkForUpdatesViewModel,
                    unsentBeforeRemoval: {
                        try await appService.unsentBeforeRemoval()
                    },
                    onForgetLibrary: { appDelegate.forgetActiveLibrary() },
                    onResetDevice: {
                        appDelegate.presentDeviceReset(
                            libraryId: appService.libraryId
                        )
                    },
                    onRestoreFromCode: {
                        appDelegate.presentDeviceRestore(
                            libraryId: appService.libraryId
                        )
                    }
                )
                .errorAlert(appDelegate.uiStore)
                .onAppear { appService.reportScreen(.settings) }
            )
        }
        else {
            ContentUnavailableView {
                Text("No library loaded")
            } description: {
                Text("Open a library first to access settings")
            }
            .frame(
                width: NoLibraryPlaceholder.width,
                height: NoLibraryPlaceholder.height
            )
        }
    }
}

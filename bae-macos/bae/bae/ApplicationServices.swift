import BaeKit
import Foundation
import Sparkle

/// bae's directory under the launch environment's `HOME`, which a UI test
/// sets to get a fresh one.
private func baeAppDir(environment: [String: String]) -> BridgeAppDir {
    guard let home = environment["HOME"], !home.isEmpty else {
        preconditionFailure("HOME is unset, so bae's directory has no location")
    }
    return BridgeAppDir(home: home)
}

/// The process's host. A UI test's launch keeps its keys and credentials in
/// an in-memory keychain, so the tests never read or write the login keychain.
@MainActor
private func makeHost(
    diagnostics: BridgeDiagnostics,
    appDir: BridgeAppDir
) -> BridgeHost {
    #if BAE_TEST_KEYRING
        if AppRuntime.usesTestKeyring(environment: baeAppProcessEnvironment) {
            return BaeHost.make {
                try BridgeHost.withTestKeyring(
                    diagnostics: diagnostics,
                    appDir: appDir
                )
            }
        }
    #endif
    return BaeHost.make(diagnostics: diagnostics, appDir: appDir)
}

/// This Mac's name as the person sees it, which other devices show for it.
private func currentDeviceName() -> String {
    guard let name = Host.current().localizedName else {
        preconditionFailure("this Mac has no computer name")
    }
    return name
}

/// The installed application's process-wide services, built together so an
/// inert preview or test host starts none of them.
@MainActor
final class ApplicationServices {
    let diagnostics: BridgeDiagnostics
    let host: BridgeHost
    let mediaControlService: MediaControlService
    /// The name this Mac registers under when it joins or sets up a library.
    let deviceName: String
    let librarySetup: LibrarySetup
    let libraryRemovals: LibraryRemovals
    let updaterController: SPUStandardUpdaterController
    let checkForUpdatesViewModel: CheckForUpdatesViewModel
    let firstResponderActions = FirstResponderActions()

    init() {
        let appDir = baeAppDir(environment: baeAppProcessEnvironment)
        diagnostics = BaeDiagnostics.configure(
            source: "macos",
            edition: baeAppEdition
        )
        host = makeHost(diagnostics: diagnostics, appDir: appDir)
        mediaControlService = MediaControlService()
        deviceName = currentDeviceName()
        librarySetup = LibrarySetup.live(host: host, deviceName: deviceName)
        libraryRemovals = LibraryRemovals.live(host: host)
        #if DEBUG
            let updaterController = SPUStandardUpdaterController(
                startingUpdater: false,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #else
            let updaterController = SPUStandardUpdaterController(
                startingUpdater: true,
                updaterDelegate: nil,
                userDriverDelegate: nil
            )
        #endif
        self.updaterController = updaterController
        checkForUpdatesViewModel = CheckForUpdatesViewModel(
            updater: updaterController.updater
        )
    }
}

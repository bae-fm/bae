import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacWelcomeHome` in the welcome window, 690 by 600.
    static let macWelcomeHomeScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.welcome-home.libraries",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(libraries: PreviewData.boardLibraries)
        },
        ShotScene(
            id: "mac.component.welcome-home.empty",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(libraries: [])
        },
        ShotScene(
            id: "mac.component.welcome-home.children",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                savedCodes: welcomeSavedCodes
            )
        },
        ShotScene(
            id: "mac.component.welcome-home.removing",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                removals: removalUnderWay
            )
        },
        ShotScene(
            id: "mac.component.welcome-home.open-failed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                loadError: homeMusicOpenFailure
            )
        },
        ShotScene(
            id: "mac.component.welcome-home.unreadable",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries + [unreadableLibrary]
            )
        },
        ShotScene(
            id: "mac.component.welcome-home.damaged-join",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                unreadableInstallation: PreviewData.unreadableInstallationId
            )
        },
    ]

    /// A removal of Home music that has left the library and goes no further.
    @MainActor
    private static var removalUnderWay: LibraryRemovals {
        let removals = PreviewData.welcomeRemovals(
            stoppedAt: leavingLibrary
        )
        removals.remove(
            closedLibrary: PreviewData.homeMusic,
            losing: .counted(holdsUnsent: false),
            onFinished: {}
        )
        return removals
    }

    /// The restore code iCloud Keychain holds for a Dropbox library.
    private static var welcomeSavedCodes: [BridgeSavedRestoreEntry] {
        [
            .readable(
                code: PreviewSavedRestoreCode(
                    BridgeSavedRestoreCodeInfo(
                        name: "Garden",
                        libraryId: "saved-garden",
                        location: .dropbox(
                            location: .appFolder,
                            accountLabel: "ana@example.com",
                            folderLabel: nil
                        ),
                        oauthAccess: .dropbox(access: .appFolder),
                        hasS3Credentials: false
                    )
                )
            )
        ]
    }
}

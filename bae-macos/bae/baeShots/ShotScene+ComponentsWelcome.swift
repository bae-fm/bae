import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the welcome window: its home in each state, the
    /// list of libraries and the first choices.
    static let welcomeComponentScenes: [ShotScene] =
        welcomeHomeScenes + welcomeLibrariesScenes + welcomeActionsScenes

    // MARK: - MacWelcomeHome

    /// `MacWelcomeHome` in the welcome window, 690 by 600.
    private static let welcomeHomeScenes: [ShotScene] = [
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

    private static let leavingLibrary = BridgeClosingProgress(
        step: .leavingLibrary,
        key: "core.library.closing_step.leaving_library"
    )

    /// Home music's open failing, in a way a reset from the cloud can fix.
    private static let homeMusicOpenFailure = LoadError(
        .libraryOpen(offersReset: true),
        PreviewData.displayErrorWithDetail,
        libraryId: PreviewData.homeMusic.id
    )

    /// A library whose storage settings bae cannot read.
    private static let unreadableLibrary = BridgeLibrary(
        id: "lib-garden",
        name: "Garden",
        path: "/Users/preview/Music/bae/Garden",
        state: .unreadable(
            error: .Diagnostic(
                category: .internal,
                detail: "storage.json: expected value at line 1"
            )
        ),
        isActive: false
    )

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

    // MARK: - MacWelcomeLibraries

    /// `MacWelcomeLibraries`, the list of this Mac's libraries with Add
    /// library… under it, in the column the welcome window gives it.
    private static let welcomeLibrariesScenes: [ShotScene] = [
        welcomeLibraries(
            "mac.component.welcome-libraries.open",
            height: 190
        ),
        welcomeLibraries(
            "mac.component.welcome-libraries.removing",
            height: 190,
            removing: true
        ),
        welcomeLibraries(
            "mac.component.welcome-libraries.open-failed",
            height: 340,
            openFailure: homeMusicOpenFailure
        ),
        welcomeLibraries(
            "mac.component.welcome-libraries.unreadable",
            height: 260,
            extra: [unreadableLibrary]
        ),
        welcomeLibraries(
            "mac.component.welcome-libraries.damaged-join",
            height: 260,
            unreadableInstallation: PreviewData.unreadableInstallationId
        ),
    ]

    private static func welcomeLibraries(
        _ id: String,
        height: CGFloat,
        extra: [BridgeLibrary] = [],
        removing: Bool = false,
        openFailure: LoadError? = nil,
        unreadableInstallation: String? = nil
    ) -> ShotScene {
        component(id, width: 420, height: height) {
            let standing = PreviewData.standingSynced
            MacWelcomeLibraries(
                libraries: PreviewData.boardLibraries + extra,
                disabled: removing,
                canDeleteActiveLibrary: true,
                removing: removing
                    ? RunningRemoval(
                        libraryId: PreviewData.homeMusic.id,
                        steps: LibraryClosingSteps(
                            host: BaeHost.inTemporaryFolder(),
                            at: leavingLibrary
                        )
                    ) : nil,
                opening: nil,
                openFailure: openFailure,
                unreadableInstallation: unreadableInstallation,
                onDiscardInstallation: { _ in },
                onResetFromCloud: { _ in },
                onOpen: { _ in },
                onFinish: { _, _ in },
                onFinishRemoving: { _ in },
                onRemove: { _ in },
                readStanding: { _ in standing },
                onCreate: {},
                onScanCode: {},
                onEnterCode: {}
            )
        }
    }

    // MARK: - MacWelcomeActions

    /// `MacWelcomeActions`, the first choices on a Mac with no library.
    private static let welcomeActionsScenes: [ShotScene] = [
        component(
            "mac.component.welcome-actions.resting",
            width: 244,
            height: 130
        ) {
            MacWelcomeActions(
                disabled: false,
                onCreate: {},
                onScanCode: {},
                onEnterCode: {}
            )
        }
    ]
}

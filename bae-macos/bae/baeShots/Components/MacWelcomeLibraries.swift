import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacWelcomeLibraries`, the list of this Mac's libraries with Add
    /// library… under it, in the column the welcome window gives it.
    static let macWelcomeLibrariesScenes: [ShotScene] = [
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
}

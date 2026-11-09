import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The welcome's list of this Mac's libraries, its first screen on a Mac
    /// that holds no library and no saved restore code, and a library's
    /// details.
    static let welcomeScenes: [ShotScene] = [
        ShotScene(
            id: "welcome-libraries",
            size: CGSize(width: 452, height: 362)
        ) {
            ShotStage(PreviewScenes.welcomeLibraries())
        },
        ShotScene(id: "story-1-first-run", size: WelcomeWindow.size) {
            let reads = CountedReads()
            return ShotStage(
                PreviewScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup
                )
            ) { reads.answered == 2 }
        },
        detailsScene("welcome-library-details", PreviewData.standingSynced),
        detailsScene(
            "welcome-library-details-unsent",
            PreviewData.standingUnsent
        ),
    ]

    /// The details popover's content for a Dropbox library, once its status
    /// is in.
    private static func detailsScene(
        _ id: String,
        _ status: BridgeLibraryStanding
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 432, height: 380)) {
            let reads = CountedReads()
            return ShotStage(
                PreviewScenes.welcomeLibraryDetails(
                    readStanding: reads.readStanding(status)
                )
            ) { reads.answered == 1 }
        }
    }
}

private struct UnservedReadError: Error {}

/// Lookups a scene's view makes on appear, counted as they are answered: the
/// view is ready once they are in.
@MainActor
@Observable
private final class CountedReads {
    private(set) var answered = 0

    var setup: LibrarySetup {
        let base = LibrarySetup.stub()
        return LibrarySetup(
            listLibraries: { [self] in
                await answer()
                return []
            },
            libraryStanding: { _ in throw UnservedReadError() },
            createLibrary: { _ in throw UnservedReadError() },
            savedRestoreCodes: { [self] in
                await answer()
                return []
            },
            forgetSavedRestoreCode: { _ in throw UnservedReadError() },
            unfinishedJoins: { [] },
            unfinishedRestores: { [] },
            makeBootstrap: {
                LibraryBootstrap(
                    host: BaeHost.preview(),
                    deviceName: "Preview Device"
                )
            },
            makeSetupSteps: { base.setupSteps() }
        )
    }

    /// A read of a closed library's status that answers `status`.
    func readStanding(_ status: BridgeLibraryStanding)
        -> @Sendable (String) async -> BridgeLibraryStanding
    {
        { [self] _ in
            await answer()
            return status
        }
    }

    private func answer() {
        answered += 1
    }
}

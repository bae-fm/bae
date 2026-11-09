import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The welcome's list of this Mac's libraries, and its first screen on a
    /// Mac that holds no library and no saved restore code.
    static let welcomeScenes: [ShotScene] = [
        ShotScene(
            id: "welcome-libraries",
            size: CGSize(width: 452, height: 362)
        ) {
            ShotStage(PreviewScenes.welcomeLibraries())
        },
        ShotScene(id: "story-1-first-run", size: WelcomeWindow.size) {
            let reads = EmptyReads()
            return ShotStage(
                PreviewScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup
                )
            ) { reads.answered == 2 }
        },
    ]
}

private struct EmptyReadsError: Error {}

/// A `LibrarySetup` whose two lists read empty, counting the lookups it has
/// answered: the first screen shows its buttons only once both are in.
@MainActor
@Observable
private final class EmptyReads {
    private(set) var answered = 0

    var setup: LibrarySetup {
        let base = LibrarySetup.stub()
        return LibrarySetup(
            listLibraries: { [self] in
                await answer()
                return []
            },
            createLibrary: { _ in throw EmptyReadsError() },
            savedRestoreCodes: { [self] in
                await answer()
                return []
            },
            forgetSavedRestoreCode: { _ in throw EmptyReadsError() },
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

    private func answer() {
        answered += 1
    }
}

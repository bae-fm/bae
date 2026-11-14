import BaeKit
import Foundation
import SwiftUI

@testable import bae

// Fixtures for the Welcome flow: a library with storage, one
// without, one that can't be read, one whose name can't be read, one whose
// restore stopped partway, and
// restore codes saved in iCloud Keychain for libraries not on this Mac,
// one of them unreadable.
// Generic placeholder names throughout.
extension PreviewData {
    static let welcomeLibraries: [BridgeLibrary] = [
        BridgeLibrary(
            id: "lib-01",
            name: "My Library",
            path: "/Users/preview/Music/bae/My Library",
            state: .openable(
                storage: .s3(
                    bucket: "music-library",
                    region: "us-east-1",
                    endpoint: nil,
                    profile: nil
                )
            ),
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
        BridgeLibrary(
            id: "lib-02",
            name: "Local Library",
            path: "/Users/preview/Music/bae/Local Library",
            state: .openable(storage: nil),
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
        BridgeLibrary(
            id: "lib-03",
            name: "Broken Library",
            path: "/Users/preview/Music/bae/Broken Library",
            state: .unreadable(
                error: .Diagnostic(
                    category: .internal,
                    detail: "storage.json: expected value at line 1"
                )
            ),
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
        BridgeLibrary(
            id: "lib-04",
            name: "Restored Library",
            path: "/Users/preview/Music/bae/Restored Library",
            state: .replacementUnfinished(
                by: .restore,
                lineKey: "core.error.replacement_unfinished.restore"
            ),
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
        BridgeLibrary(
            id: "lib-05",
            name: "Old Laptop Library",
            path: "/Users/preview/Music/bae/Old Laptop Library",
            state: .deletionUnfinished(
                lineKey: "core.error.deletion_unfinished"
            ),
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
        BridgeLibrary(
            id: "6f1c2a9e-4b7d-4e2a-9c3f-2d8e5b1a7c40",
            name: nil,
            path:
                "/Users/preview/Music/bae/6f1c2a9e-4b7d-4e2a-9c3f-2d8e5b1a7c40",
            state: .nameUnreadable,
            isActive: false,
            nextOpenableId: nil,
            previousOpenableId: nil
        ),
    ]

    /// A library stored in the person's Dropbox.
    static let homeMusic = BridgeLibrary(
        id: "lib-home-music",
        name: "Home music",
        path: "/Users/preview/Music/bae/Home music",
        state: .openable(
            storage: .dropbox(
                location: .appFolder,
                accountLabel: "mara.quill@example.com",
                folderLabel: BridgeFolderLabel(
                    name: "Music",
                    webUrl: "https://www.dropbox.com/home/Music"
                )
            )
        ),
        isActive: false,
        nextOpenableId: nil,
        previousOpenableId: nil
    )

    /// What a Dropbox library last stood at when this Mac closed it: synced
    /// two hours ago.
    static let standingSynced = BridgeLibraryStanding(
        state: .sync(
            shown: BridgeSyncShown(
                status: .synced,
                statusKey: "core.sync.synced",
                tone: .success,
                cause: nil,
                fix: nil,
                lastSynced: nil
            )
        ),
        lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
    )

    /// The same library holding changes this Mac never uploaded.
    static let standingUnsent = BridgeLibraryStanding(
        state: .unsentChanges,
        lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
    )

    /// A `LibrarySetup` whose reads serve the fixtures above — the
    /// populated welcome shots inject it, so no shot ever touches
    /// the real library directory or keychain.
    static func welcomeSetup() -> LibrarySetup {
        .stub(
            libraries: welcomeLibraries,
            savedCodes: PreviewSavedRestoreCode.entries
        )
    }

    /// Removals whose every attempt fails, for the welcome shots; with
    /// `step`, a removal started on them stays at that step and never
    /// ends.
    @MainActor
    static func welcomeRemovals(
        stoppedAt step: BridgeClosingProgress? = nil
    ) -> LibraryRemovals {
        LibraryRemovals(
            makeOpenProgress: {
                LibraryOpenProgress(host: BaeHost.inTemporaryFolder())
            },
            makeRemovalSteps: {
                step.map {
                    LibraryClosingSteps(
                        host: BaeHost.inTemporaryFolder(),
                        at: $0
                    )
                } ?? LibraryClosingSteps(host: BaeHost.inTemporaryFolder())
            },
            makeFinishingSteps: {
                LibrarySetupSteps(host: BaeHost.inTemporaryFolder())
            },
            unsentOfClosed: { _, _ in throw StubError.notImplemented },
            removeClosed: { _, _, _ in
                guard step != nil else { throw StubError.notImplemented }
                try await Task.sleep(for: .seconds(3_600))
                throw StubError.notImplemented
            },
            removeUnopenable: { _, _ in throw StubError.notImplemented },
            finishDeletion: { _, _ in throw StubError.notImplemented },
            findUnfinished: { nil },
            finishUnfinished: { _ in }
        )
    }
}

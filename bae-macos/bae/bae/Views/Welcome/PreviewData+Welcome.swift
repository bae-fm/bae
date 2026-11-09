#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the Welcome flow: a library with storage, one
    // without, one that can't be read, one whose restore stopped partway, and
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
                isActive: false
            ),
            BridgeLibrary(
                id: "lib-02",
                name: "Local Library",
                path: "/Users/preview/Music/bae/Local Library",
                state: .openable(storage: nil),
                isActive: false
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
                isActive: false
            ),
            BridgeLibrary(
                id: "lib-04",
                name: "Restored Library",
                path: "/Users/preview/Music/bae/Restored Library",
                state: .replacementUnfinished(by: .restore),
                isActive: false
            ),
            BridgeLibrary(
                id: "lib-05",
                name: "Old Laptop Library",
                path: "/Users/preview/Music/bae/Old Laptop Library",
                state: .deletionUnfinished,
                isActive: false
            ),
        ]

        /// A library stored in the person's Dropbox, for the details popover.
        static let welcomeDropboxLibrary = BridgeLibrary(
            id: "lib-06",
            name: "Low Tide Hours",
            path: "/Users/preview/Music/bae/Low Tide Hours",
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
            isActive: false
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

        /// Restore codes saved for libraries not on this Mac, one of them
        /// unreadable, as the keychain section lists them.
        static var welcomeSavedCodes: [SavedRestoreEntry] {
            PreviewSavedRestoreCode.entries.map(SavedRestoreEntry.init)
        }

        /// A `LibrarySetup` whose reads serve the fixtures above — the
        /// populated welcome previews inject it, so no preview ever touches
        /// the real library directory or keychain.
        static func welcomeSetup() -> LibrarySetup {
            .stub(
                libraries: welcomeLibraries,
                savedCodes: PreviewSavedRestoreCode.entries
            )
        }

        /// Removals whose every attempt fails, for the welcome previews.
        @MainActor
        static func welcomeRemovals() -> LibraryRemovals {
            LibraryRemovals(
                makeOpenProgress: {
                    LibraryOpenProgress(host: BaeHost.preview())
                },
                makeRemovalSteps: {
                    LibraryClosingSteps(host: BaeHost.preview())
                },
                makeFinishingSteps: {
                    LibrarySetupSteps(host: BaeHost.preview())
                },
                unsentOfClosed: { _, _ in throw StubError.notImplemented },
                removeClosed: { _, _, _ in throw StubError.notImplemented },
                removeUnopenable: { _, _ in throw StubError.notImplemented },
                finishDeletion: { _, _ in throw StubError.notImplemented },
                findUnfinished: { nil },
                finishUnfinished: { _ in }
            )
        }
    }
#endif

#if DEBUG
    import BaeKit

    // Preview fixtures for the Welcome flow: a library with a cloud home, one
    // without, one that can't be read, one whose restore stopped partway, and
    // restore codes saved in iCloud Keychain for libraries not on this Mac.
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
                        profile: nil,
                        prefix: ""
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
        ]

        /// Restore codes saved for libraries not on this Mac, as the
        /// keychain section lists them.
        static var welcomeSavedCodes: [SavedRestoreEntry] {
            PreviewSavedRestoreCode.samples.map {
                SavedRestoreEntry(code: PreviewSavedRestoreCode($0))
            }
        }

        /// A `LibrarySetup` whose reads serve the fixtures above — the
        /// populated welcome previews inject it, so no preview ever touches
        /// the real library directory or keychain.
        static func welcomeSetup() -> LibrarySetup {
            .stub(
                libraries: welcomeLibraries,
                savedCodes: PreviewSavedRestoreCode.samples
            )
        }

        /// Removals whose every attempt fails, for the welcome previews.
        @MainActor
        static func welcomeRemovals() -> LibraryRemovals {
            LibraryRemovals(
                unsentOfClosed: { _ in throw StubError.notImplemented },
                removeClosed: { _ in throw StubError.notImplemented },
                removeUnopenable: { _ in throw StubError.notImplemented },
                findUnfinished: { nil },
                finishUnfinished: {}
            )
        }
    }
#endif

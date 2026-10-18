#if DEBUG
    import BaeKit

    // Preview fixtures for the Welcome flow: a library with a cloud home and
    // one without. Saved restore codes are opaque bridge objects a preview
    // can't make, so the populated previews show none. Generic placeholder
    // names throughout.
    extension PreviewData {
        static let welcomeLibraries: [BridgeLibrary] = [
            BridgeLibrary(
                id: "lib-01",
                name: "My Library",
                path: "/Users/preview/Music/bae/My Library",
                settings: .readable(
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
                settings: .readable(storage: nil),
                isActive: false
            ),
            BridgeLibrary(
                id: "lib-03",
                name: "Broken Library",
                path: "/Users/preview/Music/bae/Broken Library",
                settings: .unreadable(
                    error: .Diagnostic(
                        category: .internal,
                        detail: "storage.json: expected value at line 1"
                    )
                ),
                isActive: false
            ),
        ]

        /// A `LibrarySetup` whose reads serve the fixtures above — the
        /// populated welcome previews inject it, so no preview ever touches
        /// the real library directory or keychain.
        static func welcomeSetup() -> LibrarySetup {
            .stub(libraries: welcomeLibraries)
        }

        /// Removals whose every attempt fails, for the welcome previews.
        @MainActor
        static func welcomeRemovals() -> LibraryRemovals {
            LibraryRemovals(
                unsentOfClosed: { _ in throw StubError.notImplemented },
                removeClosed: { _ in throw StubError.notImplemented },
                findUnfinished: { nil },
                finishUnfinished: {}
            )
        }
    }
#endif

#if DEBUG
    import BaeKit

    // Preview fixtures for the Welcome flow's leaf sections — a couple of
    // libraries (one healthy, one that won't load), keychain restore entries
    // (one plain, one needing OAuth). Generic placeholder names throughout.
    extension PreviewData {
        static let welcomeLibraries: [BridgeLibrary] = [
            BridgeLibrary(
                id: "lib-01",
                name: "My Library",
                path: "/Users/preview/Music/bae/My Library",
                cloudProvider: .s3,
                isActive: false,
                error: nil,
            ),
            // A broken library's `name` is its directory id — the name is
            // exactly what couldn't be read — so the fixture mirrors that: the
            // id stands in for the name, and the path points at that folder.
            BridgeLibrary(
                id: "9f3a2b10c4d8",
                name: "9f3a2b10c4d8",
                path: "/Users/preview/Music/bae/9f3a2b10c4d8",
                cloudProvider: nil,
                isActive: false,
                error: "config.yaml could not be read",
            ),
        ]

        static let welcomeKeychainEntries:
            [(code: String, info: BridgeRestoreCodeInfo)] = [
                (
                    code: "restore-code-plain",
                    info: BridgeRestoreCodeInfo(
                        libraryId: "lib-03",
                        libraryName: "Cloud Library",
                        cloudProvider: .s3,
                        needsOauth: false,
                    )
                ),
                (
                    code: "restore-code-oauth",
                    info: BridgeRestoreCodeInfo(
                        libraryId: "lib-04",
                        libraryName: "Shared Library",
                        cloudProvider: .googleDrive,
                        needsOauth: true,
                    )
                ),
            ]

        /// A `LibrarySetup` whose reads serve the fixtures above — the
        /// populated welcome previews inject it, so no preview ever touches
        /// the real library directory or keychain.
        static func welcomeSetup() -> LibrarySetup {
            LibrarySetup(
                discoverLibraries: { welcomeLibraries },
                decodeRestoreCode: { code in
                    guard
                        let entry = welcomeKeychainEntries.first(where: {
                            $0.code == code
                        })
                    else {
                        throw StubError.notImplemented
                    }
                    return entry.info
                },
                fetchRestoreCodes: {
                    welcomeKeychainEntries.map {
                        (libraryId: $0.info.libraryId, code: $0.code)
                    }
                },
            )
        }
    }
#endif

#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the Welcome flow's leaf sections — a couple of
    // libraries (one healthy, one that won't load), keychain restore entries
    // (one plain, one needing OAuth), and the join-flow codes. Generic
    // placeholder names throughout.
    extension PreviewData {
        static let welcomeLibraries: [BridgeLibrary] = [
            BridgeLibrary(
                id: "lib-01",
                name: "My Library",
                path: "/preview/My Library",
                cloudProvider: .s3,
                isActive: false,
                error: nil,
            ),
            BridgeLibrary(
                id: "lib-02",
                name: "lib-02",
                path: "/preview/broken",
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

        static let welcomeJoinRequest = BridgeJoinRequest(
            code: "join-request-code",
            fingerprint: "a1b2c3d4",
        )

        static let welcomeInviteInfo = BridgeInviteCodeInfo(
            libraryId: "lib-05",
            libraryName: "My Library",
            ownerPubkey: "00112233445566778899aabbccddeeff",
            ownerFingerprint: "00112233",
            cloudProvider: .s3,
            needsOauth: false,
        )

        /// An invite for an OAuth-backed library — previews the mismatch
        /// warning shown when the joiner hasn't signed in to that provider.
        static let welcomeInviteInfoOauth = BridgeInviteCodeInfo(
            libraryId: "lib-06",
            libraryName: "Shared Library",
            ownerPubkey: "ffeeddccbbaa99887766554433221100",
            ownerFingerprint: "ffeeddcc",
            cloudProvider: .googleDrive,
            needsOauth: true,
        )

        /// A generic failure whose `displayLine` renders in previews.
        static func welcomeFailure(_ message: String) -> Error {
            NSError(
                domain: "preview",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: message]
            )
        }
    }
#endif

#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the Library settings flow — a membership chain (this
    // device as owner plus one removable member), a decoded join request, a
    // `Sync` whose reads serve those fixtures, and connected/erroring config
    // stores. Generic placeholder identities throughout; the pubkeys and
    // fingerprints are arbitrary hex.
    extension PreviewData {
        // MARK: - Membership

        static let members: [BridgeMember] = [
            BridgeMember(
                pubkey:
                    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
                role: .owner,
                isSelf: true,
                fingerprint: "00112233",
                canRemove: false,
            ),
            BridgeMember(
                pubkey:
                    "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100",
                role: .member,
                isSelf: false,
                fingerprint: "ffeeddcc",
                canRemove: true,
            ),
        ]

        static let membership = BridgeMembership(
            members: members,
            selfIsOwner: true,
        )

        /// A `Sync` whose reads serve the membership fixtures above, so the
        /// members and library-settings previews never touch a real membership
        /// chain or cloud storage.
        static let previewSync = Sync(
            generateRestoreCode: { "recovery-code-preview" },
            getMembers: { membership },
            inviteMember: { _, _ in "invite-code-preview" },
            cloudOnlyReleaseCount: { 0 },
        )

        // MARK: - Connected / erroring config

        static let previewSyncConfig = BridgeSyncConfig(
            provider: .s3(
                bucket: "preview-bucket",
                region: "us-east-1",
                endpoint: nil,
            ),
            cloudAccountDisplay: "s3://preview-bucket",
        )

        /// A `ConfigStore` reporting a live sync connection (a provider is
        /// configured and the loop is up), so the Library settings preview shows
        /// the connected controls, the devices list, and the recovery section.
        @MainActor
        static let connectedConfigStore = ConfigStore(
            config: Config(bridge: connectedBridgeConfig),
            syncReady: true,
        )

        /// A `ConfigStore` whose sync loop has reported an error — the Sync
        /// section's reconnect banner reads `syncError` directly.
        @MainActor
        static let syncErrorConfigStore: ConfigStore = {
            let store = makeConfigStore(libraryFullWidth: false)
            store.syncError = DisplayError(
                line:
                    "The cloud provider rejected the request (403 Forbidden)."
            )
            return store
        }()

        private static let connectedBridgeConfig = BridgeConfig(
            libraryId: "lib-preview",
            libraryName: "Preview Library",
            libraryPath: "/preview",
            encryptionKeyStored: true,
            encryptionKeyFingerprint: "abcd1234",
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: savePresets,
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4533,
                username: "",
                bindAddress: "127.0.0.1",
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false,
            sync: previewSyncConfig,
        )
    }
#endif

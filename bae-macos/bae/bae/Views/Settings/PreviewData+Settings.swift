#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the settings flows — the save-format presets and
    // their filename tokens, plus the Library membership chain (this device as
    // owner plus one removable member), a `Sync` whose reads serve those
    // fixtures, and connected/erroring config stores. Generic
    // placeholder identities throughout; the pubkeys and fingerprints are
    // arbitrary hex.
    extension PreviewData {
        // MARK: - Formats

        /// Default single-track export filename pattern, for seeding preview
        /// configs (mirrors bae-core's `default_save_filename_tokens`).
        static let exportFilenameTokens: [BridgeSaveFilenameToken] = [
            .trackNumber, .title,
        ]
        static let savePresets: [BridgeSavePreset] = [
            BridgeSavePreset(
                id: "flac",
                name: "FLAC",
                codec: .flac(bitDepth: .source),
                extension: "flac",
                filenameTokens: exportFilenameTokens,
                pregapPlacement: .appendToPreviousExceptHtoa,
                appliesToTrack: true,
                appliesToRelease: true,
                embedCover: true
            ),
            BridgeSavePreset(
                id: "mp3",
                name: "MP3",
                codec: .mp3(bitrateKbps: 320),
                extension: "mp3",
                filenameTokens: exportFilenameTokens,
                pregapPlacement: .appendToPreviousExceptHtoa,
                appliesToTrack: true,
                appliesToRelease: true,
                embedCover: true
            ),
        ]

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
        static func previewSync() -> Sync {
            Sync(
                generateRestoreCode: { "recovery-code-preview" },
                getMembers: { membership },
                cloudOnlyReleaseCount: { 0 },
            )
        }

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
        static func connectedConfigStore() -> ConfigStore {
            ConfigStore(config: Config(bridge: connectedBridgeConfig))
        }

        private static let connectedBridgeConfig = BridgeConfig(
            libraryId: "lib-preview",
            libraryName: "Preview Library",
            libraryPath: "/preview",
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            identifyAutomatically: true,
            defaultImportMetadataSource: .findOnline,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: savePresets,
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
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

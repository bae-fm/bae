#if DEBUG
    import BaeKit
    import Foundation

    // Preview fixtures for the settings flows — the save-format presets and
    // their filename tokens, plus the Library people (this device's person as
    // admin plus one removable member), a `Sync` whose reads serve preview
    // values, and a connected config store. Generic placeholder identities
    // throughout.
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

        static let previewStorage = BridgeStorageLocation.s3(
            bucket: "preview-bucket",
            region: "us-east-1",
            endpoint: nil,
            profile: nil,
            prefix: ""
        )

        /// This device's person, an admin, and one other member with a
        /// removable device. Ids are arbitrary UUIDs.
        static let members: [BridgePerson] = [
            BridgePerson(
                id: "4a1f2c3d-0000-4000-8000-000000000001",
                profile: BridgePersonProfile(
                    id: "4a1f2c3d-0000-4000-8000-000000000001",
                    name: "Person A",
                    privateCircle: "circle-a",
                    historySharing: .private,
                    avatarId: nil
                ),
                membership: BridgeMember(
                    id: "4a1f2c3d-0000-4000-8000-000000000001",
                    keys: BridgeMemberPublicKeys(
                        signing: Data(),
                        sealing: Data()
                    ),
                    role: .admin,
                    devices: [
                        BridgeDevice(
                            id: 1,
                            name: "Person A's Mac",
                            state: .active,
                            isSelf: true
                        )
                    ],
                    isSelf: true,
                    canRemove: false,
                    canRemoveDevices: true
                )
            ),
            BridgePerson(
                id: "4a1f2c3d-0000-4000-8000-000000000002",
                profile: BridgePersonProfile(
                    id: "4a1f2c3d-0000-4000-8000-000000000002",
                    name: "Person B",
                    privateCircle: "circle-b",
                    historySharing: .household,
                    avatarId: nil
                ),
                membership: BridgeMember(
                    id: "4a1f2c3d-0000-4000-8000-000000000002",
                    keys: BridgeMemberPublicKeys(
                        signing: Data(),
                        sealing: Data()
                    ),
                    role: .member,
                    devices: [
                        BridgeDevice(
                            id: 2,
                            name: "Person B's Phone",
                            state: .active,
                            isSelf: false
                        )
                    ],
                    isSelf: false,
                    canRemove: true,
                    canRemoveDevices: true
                )
            ),
        ]

        /// A `Sync` whose reads serve preview values, so the library-settings
        /// previews never touch cloud storage.
        static func previewSync() -> Sync {
            Sync(
                generateRestoreCode: {
                    BridgeRestoreCodeShare(
                        code: "restore-code-preview",
                        qrText: "restore-code-preview",
                        qrCarriesS3Key: false
                    )
                },
                cloudOnlyReleaseCount: { 0 }
            )
        }

        /// A `ConfigStore` whose library has a cloud home, so the Library
        /// settings preview shows the connected controls and the people.
        @MainActor
        static func connectedConfigStore() -> ConfigStore {
            makeConfigStore(libraryFullWidth: false, storage: previewStorage)
        }
    }
#endif

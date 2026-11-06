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
        /// configs (mirrors bae-library's `default_save_filename_tokens`).
        static let exportFilenameTokens: [BridgeSaveFilenameToken] = [
            .trackNumber, .title,
        ]
        static let savePresets: [BridgeSavePreset] = [
            BridgeSavePreset(
                id: "flac",
                name: "FLAC",
                codec: .flac(bitDepth: .bits24),
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
            profile: nil
        )

        /// A Dropbox shared folder, with the account and the folder's label
        /// coven read at the latest connection.
        static let previewDropboxStorage = BridgeStorageLocation.dropbox(
            location: .sharedFolder(namespaceId: "1234567890"),
            accountLabel: "name@example.com",
            folderLabel: BridgeFolderLabel(
                name: "Music",
                webUrl: "https://www.dropbox.com/home/Music"
            )
        )

        /// A OneDrive folder, with the account and the folder's label coven
        /// read at the latest connection.
        static let previewOneDriveStorage = BridgeStorageLocation.oneDrive(
            driveId: "drive-1",
            folderId: "folder-1",
            accountLabel: "name@example.com",
            folderLabel: BridgeFolderLabel(
                name: "Music Library",
                webUrl: "https://onedrive.live.com/?id=folder-1"
            )
        )

        /// This device's person, an admin, whose earlier registration of this
        /// device was replaced, and one other member with a removable device
        /// and two removed ones. Ids are arbitrary UUIDs.
        static let members: [BridgePerson] = [
            BridgePerson(
                profile: BridgePersonProfile(
                    id: "4a1f2c3d-0000-4000-8000-000000000001",
                    name: "Person A",
                    privateCircle: "circle-a",
                    historySharing: .private,
                    avatarVersion: nil
                ),
                membership: BridgeMember(
                    id: "4a1f2c3d-0000-4000-8000-000000000001",
                    keys: BridgeMemberPublicKeys(
                        signing: Data(),
                        sealing: Data()
                    ),
                    role: .admin,
                    access: .s3AccessKey(accessKeyId: "AKIAPREVIEWKEY"),
                    isSelf: true,
                    canRemove: false,
                    activeDevices: [
                        BridgeActiveDevice(
                            id: 3,
                            name: "Person A's Mac",
                            isSelf: true,
                            canRemove: false
                        )
                    ],
                    formerDevices: [
                        BridgeFormerDevice(
                            id: 1,
                            name: "Person A's Mac",
                            state: .replaced
                        )
                    ]
                )
            ),
            BridgePerson(
                profile: BridgePersonProfile(
                    id: "4a1f2c3d-0000-4000-8000-000000000002",
                    name: "Person B",
                    privateCircle: "circle-b",
                    historySharing: .household,
                    avatarVersion: nil
                ),
                membership: BridgeMember(
                    id: "4a1f2c3d-0000-4000-8000-000000000002",
                    keys: BridgeMemberPublicKeys(
                        signing: Data(),
                        sealing: Data()
                    ),
                    role: .member,
                    access: .s3AccessKey(accessKeyId: "AKIAPREVIEWKEY"),
                    isSelf: false,
                    canRemove: true,
                    activeDevices: [
                        BridgeActiveDevice(
                            id: 2,
                            name: "Person B's Phone",
                            isSelf: false,
                            canRemove: true
                        )
                    ],
                    formerDevices: [
                        BridgeFormerDevice(
                            id: 4,
                            name: "Person B's Old Laptop",
                            state: .removed
                        ),
                        BridgeFormerDevice(
                            id: 5,
                            name: "Person B's Tablet",
                            state: .removed
                        ),
                    ]
                )
            ),
        ]

        /// A recovery code shaped like coven's: `CVR1-` and uppercase base32.
        static let recoveryCode =
            "CVR1-"
            + String(repeating: "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567", count: 6)

        /// A `Sync` whose reads serve preview values, so the library-settings
        /// previews never touch cloud storage.
        static func previewSync() -> Sync {
            Sync(
                storageSetupSteps: {
                    StorageSetupSteps(host: BaeHost.preview())
                },
                generateRestoreCode: {
                    BridgeRestoreCodeShare(
                        code: recoveryCode,
                        qrText: recoveryCode,
                        qrCarriesS3Key: false
                    )
                },
                addDevice: {
                    BridgeAddDevice(
                        share: BridgeRestoreCodeShare(
                            code: recoveryCode,
                            qrText: recoveryCode,
                            qrCarriesS3Key: false
                        ),
                        watch: BridgeAddedDeviceWatch(
                            noHandle: BridgeAddedDeviceWatch.NoHandle()
                        )
                    )
                }
            )
        }

        /// A `ConfigStore` whose library has its cloud home at `storage`, so
        /// the Library settings preview shows the storage, its sync and the
        /// people.
        @MainActor
        static func connectedConfigStore(
            storage: BridgeStorageLocation
        ) -> ConfigStore {
            makeConfigStore(libraryFullWidth: false, storage: storage)
        }
    }
#endif

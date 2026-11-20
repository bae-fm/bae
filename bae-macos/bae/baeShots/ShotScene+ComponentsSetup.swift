import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of a library's storage setup and its Settings
    /// rows: the S3 form and its key fields, the folder picker, the invite
    /// form, the sync status and the restore code.
    static let setupComponentScenes: [ShotScene] =
        s3FormScenes + s3KeyFieldsScenes + folderPickerScenes
        + inviteFieldsScenes + syncStatusScenes + restoreCodeScenes

    // MARK: - MacS3Form

    /// `MacS3Form` with the preview's endpoint, region, bucket and key typed
    /// in, and the reason a failed setup gives under the field it names.
    private static let s3FormScenes: [ShotScene] = [
        s3Form("mac.component.s3-form.none", failing: nil),
        s3Form("mac.component.s3-form.region", failing: .regionMismatch),
        s3Form("mac.component.s3-form.bucket", failing: .containerNotFound),
        s3Form("mac.component.s3-form.occupied", failing: .ownLocationRequired),
    ]

    private static func s3Form(
        _ id: String,
        failing failure: BridgeStorageFailure?
    ) -> ShotScene {
        component(id, width: 520, height: 470, inset: 0) {
            MacS3Form(staged: s3Fields(failing: failure))
                .environment(Sync.stub())
                .environment(StorageSetupRun())
        }
    }

    @MainActor
    private static func s3Fields(
        failing failure: BridgeStorageFailure?
    ) -> S3SetupFields {
        let fields = S3SetupFields()
        fields.endpoint = "https://s3.home.lan"
        fields.region = "us-east-1"
        fields.bucket = "my-bucket"
        fields.accessKey = "a1b2c3d4e5f6"
        fields.secretKey = "abcdefghijkl"
        if let failure {
            _ = fields.place(
                BridgeError.Diagnostic(
                    category: .storage(failure: failure),
                    detail: "the storage answered with an error"
                )
            )
        }
        return fields
    }

    // MARK: - MacS3KeyFields

    /// `MacS3KeyFields` empty, typed in and refused. The ID field takes the
    /// focus while it is empty and the secret field once the ID is typed, so
    /// the preview's "ID focused" with both typed has no state of the view.
    private static let s3KeyFieldsScenes: [ShotScene] = [
        s3KeyFields("mac.component.s3-key-fields.empty", height: 130),
        s3KeyFields(
            "mac.component.s3-key-fields.typed",
            height: 130,
            accessKey: "AKIA4EXAMPLENEW",
            secretKey: "abcdefghijkl"
        ),
        s3KeyFields(
            "mac.component.s3-key-fields.refused",
            height: 200,
            accessKey: "AKIA4EXAMPLENEW",
            secretKey: "abcdefghijkl",
            refused: true
        ),
    ]

    private static func s3KeyFields(
        _ id: String,
        height: CGFloat,
        accessKey: String = "",
        secretKey: String = "",
        refused: Bool = false
    ) -> ShotScene {
        component(id, width: 420, height: height) {
            MacS3KeyFields(
                accessKey: .constant(accessKey),
                secretKey: .constant(secretKey)
            ) {
                if refused {
                    MacNotice(
                        title: String(localized: "Could not use this key"),
                        advice: String(
                            localized:
                                "S3 storage does not accept this access key."
                        ),
                        error: nil,
                        look: .quiet
                    )
                }
            }
        }
    }

    // MARK: - MacFolderPicker

    #if BAE_OAUTH_PROVIDERS
        /// `MacFolderPicker` in the setup sheet that holds it, with the New
        /// folder… and Look again buttons and Cancel and Set up in its
        /// footer.
        private static let folderPickerScenes: [ShotScene] = [
            folderPicker(
                "mac.component.folder-picker.naming",
                dropbox(["Family", "Music", "Photos"]),
                isNaming: true,
                newFolderName: "bae"
            ),
            folderPicker(
                "mac.component.folder-picker.list",
                dropbox(["Family", "Music", "Photos"]),
                selected: "Music"
            ),
            folderPicker(
                "mac.component.folder-picker.made",
                dropbox(["Family", "Music", "Photos", "bae"]),
                selected: "bae"
            ),
            folderPicker(
                "mac.component.folder-picker.not-empty",
                dropbox(["Family", "Music", "Photos"]),
                selected: "Music",
                notEmpty: "Music"
            ),
            folderPicker(
                "mac.component.folder-picker.empty",
                dropbox([])
            ),
            folderPicker(
                "mac.component.folder-picker.onedrive",
                oneDrive(["Live", "Albums", "Mixes"]),
                selected: "Albums"
            ),
        ]

        private static func dropbox(_ names: [String]) -> FolderListing {
            FolderListing(
                provider: .dropbox,
                path: [],
                folders: names.map {
                    BridgeStorageFolder(
                        name: $0,
                        location: .dropbox(
                            location: .sharedFolder(namespaceId: $0),
                            accountLabel: nil,
                            folderLabel: nil
                        )
                    )
                }
            )
        }

        private static func oneDrive(_ names: [String]) -> FolderListing {
            func folder(_ name: String) -> BridgeStorageFolder {
                BridgeStorageFolder(
                    name: name,
                    location: .oneDrive(
                        driveId: "drive",
                        folderId: name,
                        accountLabel: nil,
                        folderLabel: nil
                    )
                )
            }
            return FolderListing(
                provider: .oneDrive,
                path: [folder("OneDrive"), folder("Music")],
                folders: names.map(folder)
            )
        }

        /// The folders a provider lists, and the path to them.
        private struct FolderListing {
            let provider: StorageFolderBrowser.Provider
            let path: [BridgeStorageFolder]
            let folders: [BridgeStorageFolder]
        }

        private static func folderPicker(
            _ id: String,
            _ listing: FolderListing,
            selected: String? = nil,
            isNaming: Bool = false,
            newFolderName: String = "",
            notEmpty: String? = nil
        ) -> ShotScene {
            component(id, width: 480, height: 440, inset: 0) {
                let named = { (name: String?) in
                    listing.folders.first { $0.name == name }
                }
                OAuthFolderSetup(
                    browser: StorageFolderBrowser(
                        staged: listing.provider,
                        listing: StorageFolderBrowser.Listing(
                            path: listing.path,
                            folders: listing.folders
                        ),
                        nests: listing.provider == .oneDrive,
                        selected: named(selected),
                        isNaming: isNaming,
                        newFolderName: newFolderName,
                        notEmpty: named(notEmpty)
                    ),
                    leave: {},
                    onConnected: { _ in }
                )
                .environment(Sync.stub())
                .environment(StorageSetupRun())
                .environment(CloudSyncSetup())
            }
        }
    #else
        private static let folderPickerScenes: [ShotScene] = []
    #endif

    // MARK: - MacInviteFields

    /// `MacInviteFields` for each storage, in its sheet.
    private static let inviteFieldsScenes: [ShotScene] = [
        inviteFields("mac.component.invite-fields.s3-their-key", .s3),
        inviteFields(
            "mac.component.invite-fields.s3-my-key",
            .s3,
            usesOwnKey: true
        ),
        inviteFields("mac.component.invite-fields.dropbox", .dropbox),
        inviteFields("mac.component.invite-fields.onedrive", .oneDrive),
        inviteFields("mac.component.invite-fields.icloud", .iCloud),
    ]

    private static func inviteFields(
        _ id: String,
        _ storage: InviteStorage,
        usesOwnKey: Bool = false
    ) -> ShotScene {
        component(id, width: 456, height: 400) {
            MacInviteFields(
                storage: storage,
                email: .constant("dan@example.com"),
                accessKeyId: .constant("AKIAEXAMPLEKEY"),
                secretAccessKey: .constant("abcdefghijkl"),
                usesOwnKey: .constant(usesOwnKey),
                role: .constant(.member),
                lifetime: .constant(.sevenDays),
                error: nil,
                canCreate: true,
                onCreate: {},
                onCancel: {}
            )
        }
    }

    // MARK: - MacSyncStatus

    /// `MacSyncStatus` for storage that refuses this Mac's S3 key. The
    /// preview's remedy line, what bae does next, has no line in the view:
    /// the cause is the one line core gives.
    private static let syncStatusScenes: [ShotScene] = [
        syncStatus("mac.component.sync-status.status", height: 100),
        syncStatus(
            "mac.component.sync-status.no-tone",
            height: 100,
            tone: .neutral
        ),
        syncStatus(
            "mac.component.sync-status.cause",
            height: 150,
            cause: .changesStayOnDevice
        ),
        syncStatus(
            "mac.component.sync-status.fix",
            height: 150,
            fix: .enterAccessKey
        ),
        syncStatus(
            "mac.component.sync-status.last-synced",
            height: 140,
            lastSynced: Date(timeIntervalSinceNow: -3 * 86_400)
        ),
    ]

    private static func syncStatus(
        _ id: String,
        height: CGFloat,
        tone: BridgeSyncTone = .warning,
        cause: BridgeSyncCause? = nil,
        fix: BridgeSyncFix? = nil,
        lastSynced: Date? = nil
    ) -> ShotScene {
        component(id, width: 500, height: height) {
            ShotBox {
                MacSyncStatus(
                    location: nil,
                    actions: SyncFixActions(
                        onResetDevice: {},
                        onRestoreFromCode: {},
                        onCheckForUpdates: {},
                        onRemoveLibrary: {},
                        onEnterAccessKey: {}
                    )
                )
            }
            .environment(
                SyncStatusStore(
                    snapshot: BridgeSyncStatusSnapshot(
                        shown: BridgeSyncShown(
                            status: .signedOutOfS3,
                            statusKey: "core.sync.status.signed_out_s3",
                            shortStatusKey: "core.sync.status.signed_out",
                            tone: tone,
                            cause: cause,
                            fix: fix,
                            lastSynced: lastSynced
                        ),
                        setupRunning: false,
                        hasCloudHome: true,
                        undoneSetup: nil
                    )
                )
            )
            .environment(Sync.stub())
            #if BAE_OAUTH_PROVIDERS
                .environment(CloudSyncSetup())
            #endif
        }
    }

    // MARK: - MacRestoreCode

    /// `MacRestoreCode` hidden, shown, and for an S3 library, whose key the
    /// code does not carry.
    private static let restoreCodeScenes: [ShotScene] = [
        restoreCode(
            "mac.component.restore-code.hidden",
            height: 150,
            state: .masked(PreviewData.restoreCodeShare)
        ),
        restoreCode(
            "mac.component.restore-code.shown",
            height: 170,
            state: .shown(PreviewData.restoreCodeShare)
        ),
        restoreCode(
            "mac.component.restore-code.s3",
            height: 190,
            state: .masked(PreviewData.restoreCodeShare),
            storage: .s3(
                bucket: "my-bucket",
                region: "us-east-1",
                endpoint: "https://s3.home.lan",
                profile: nil
            )
        ),
    ]

    private static func restoreCode(
        _ id: String,
        height: CGFloat,
        state: RestoreCodeReveal.State,
        storage: BridgeStorageLocation? = nil
    ) -> ShotScene {
        component(id, width: 500, height: height) {
            ShotForm {
                MacRestoreCode(
                    reveal: RestoreCodeReveal(previewState: state),
                    onPrint: { _ in }
                )
            }
            .environment(
                LibrarySettingsTabPreviewFixture(
                    storage: storage ?? PreviewData.previewDropboxStorage
                )
                .configStore
            )
        }
    }
}

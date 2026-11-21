import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The setup sheet's boards (`mac.settings.sync-setup.*`): the sheet over
    /// the Settings window's Library pane, which shows a library with no
    /// cloud home, on each screen of each provider's setup. After setup the
    /// restore code's screen hangs over the Sync page, and the Done boards
    /// show that page alone.
    static let syncSetupScenes: [ShotScene] =
        syncSetupAllScenes + syncSetupOAuthScenes + syncSetupCloudKitScenes

    private static let syncSetupAllScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.sync-setup.s3",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(nil)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-region-mismatch",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(.regionMismatch)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-bucket-not-found",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(.containerNotFound)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-bucket-not-empty",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(.ownLocationRequired)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-access-denied",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(nil, rejection: .cannotUseBucket)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-key-not-accepted",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Setup(nil, rejection: .notAccepted)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-step-1",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            running(.s3, .checkingLocation, finishing: false, s3: true)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-step-2",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            running(.s3, .detectingSettings, finishing: false, s3: true)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-step-3",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            running(.s3, .connecting, finishing: false, s3: true)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-step-4",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            running(.s3, .creatingStore, finishing: true, s3: true)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-choose-home",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            providers(.s3)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-restore-code",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            restoreCode(.s3, connected: s3Storage)
        },
        ShotScene(
            id: "mac.settings.sync-setup.s3-done",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            syncPage(s3Storage)
        },
    ]

    #if BAE_OAUTH_PROVIDERS
        private static let syncSetupOAuthScenes: [ShotScene] = [
            ShotScene(
                id: "mac.settings.sync-setup.choose-cloud",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                providers(.cloudKit)
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-signing-in",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .signingIn))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-reading-folders",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .reading))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-choose-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .list))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-naming-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .naming))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-folder-made",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .made))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-folder-not-empty",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .notEmpty))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-no-folders",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .empty))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-making-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.dropbox, oauthBrowser(.dropbox, .making))
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-checking-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .dropbox,
                    .checkingLocation,
                    browser: oauthBrowser(.dropbox, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-step-1",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .dropbox,
                    .readingAccount,
                    browser: oauthBrowser(.dropbox, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-step-2",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .dropbox,
                    .connecting,
                    browser: oauthBrowser(.dropbox, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-step-3",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .dropbox,
                    .creatingStore,
                    finishing: true,
                    browser: oauthBrowser(.dropbox, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-choose-home",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                providers(.dropbox)
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-restore-code",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                restoreCode(.dropbox, connected: dropboxStorage)
            },
            ShotScene(
                id: "mac.settings.sync-setup.dropbox-done",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                syncPage(dropboxStorage)
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-signing-in",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .signingIn))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-reading-folders",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .reading))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-choose-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .list))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-naming-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .naming))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-folder-made",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .made))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-folder-not-empty",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .notEmpty))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-no-folders",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .empty))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-making-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.oneDrive, oauthBrowser(.oneDrive, .making))
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-checking-folder",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .oneDrive,
                    .checkingLocation,
                    browser: oauthBrowser(.oneDrive, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-step-1",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .oneDrive,
                    .readingAccount,
                    browser: oauthBrowser(.oneDrive, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-step-2",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .oneDrive,
                    .connecting,
                    browser: oauthBrowser(.oneDrive, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-step-3",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(
                    .oneDrive,
                    .creatingStore,
                    finishing: true,
                    browser: oauthBrowser(.oneDrive, .list)
                )
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-choose-home",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                providers(.oneDrive)
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-restore-code",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                restoreCode(.oneDrive, connected: oneDriveStorage)
            },
            ShotScene(
                id: "mac.settings.sync-setup.onedrive-done",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                syncPage(oneDriveStorage)
            },
        ]
    #else
        private static let syncSetupOAuthScenes: [ShotScene] = []
    #endif

    #if BAE_CLOUDKIT
        private static let syncSetupCloudKitScenes: [ShotScene] = [
            ShotScene(
                id: "mac.settings.sync-setup.icloud",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                choosing(.cloudKit, nil)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-step-1",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(.cloudKit, .preparingZone, finishing: false)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-step-2",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(.cloudKit, .connecting, finishing: false)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-step-3",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                running(.cloudKit, .creatingStore, finishing: true)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-choose-home",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                providers(.cloudKit)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-restore-code",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                restoreCode(.cloudKit, connected: icloudStorage)
            },
            ShotScene(
                id: "mac.settings.sync-setup.icloud-done",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                syncPage(icloudStorage)
            },
        ]
    #else
        private static let syncSetupCloudKitScenes: [ShotScene] = []
    #endif

    // MARK: - Staging

    private static let dropboxStorage = PreviewData.previewDropboxStorage

    private static let oneDriveStorage = BridgeStorageLocation.oneDrive(
        driveId: "drive-1",
        folderId: "folder-1",
        accountLabel: "name@example.com",
        folderLabel: BridgeFolderLabel(
            name: "Music",
            webUrl: "https://onedrive.live.com/?id=folder-1"
        )
    )

    private static let s3Storage = BridgeStorageLocation.s3(
        bucket: "my-bucket",
        region: "us-east-1",
        endpoint: "https://s3.home.lan",
        profile: nil
    )

    private static let icloudStorage = BridgeStorageLocation.cloudKit(
        container: "iCloud.fm.bae",
        owner: "owner",
        zone: "zone"
    )

    /// The sync status line of a library whose setup just ended.
    private static let syncing = BridgeSyncShown(
        status: .syncing,
        statusKey: "core.sync.syncing",
        shortStatusKey: "core.sync.syncing",
        tone: .neutral,
        cause: nil,
        fix: nil,
        lastSynced: nil
    )

    /// The wizard hung as a sheet over the Library pane of a library with no
    /// cloud home; or, when `connected` names the storage setup just
    /// connected, over the Sync page of that library.
    @MainActor
    private static func sheet(
        _ staging: SyncSetupWizardStaging,
        connected: BridgeStorageLocation? = nil
    ) -> ShotStage {
        let fixture = LibrarySettingsTabPreviewFixture(storage: connected)
        let wizard = SyncSetupWizard(staging: staging)
            .environment(PreviewData.previewSync())
            .environment(fixture.configStore)
        #if BAE_OAUTH_PROVIDERS
            let dialog = wizard.environment(CloudSyncSetup())
        #else
            let dialog = wizard
        #endif
        return ShotStage(
            OverWindow.sheet(
                dialog,
                over: connected.map { syncPageWindow($0) } ?? libraryWindow()
            )
        )
    }

    @MainActor
    private static func libraryWindow() -> AnyView {
        let fixture = LibrarySettingsTabPreviewFixture(storage: nil)
        let tab = LibrarySettingsTab(
            onResetDevice: {},
            onRestoreFromCode: {},
            onCheckForUpdates: {},
            onRenameLibrary: {},
            onRemoveLibrary: {}
        )
        .environment(PreviewData.previewSync())
        .environment(fixture.configStore)
        .environment(fixture.membership)
        .environment(UiStore())
        .environment(fixture.libraryName)
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
        return AnyView(
            SettingsWindowShot(shown: .fixtureNotSynced, page: tab)
        )
    }

    @MainActor
    private static func syncPageWindow(
        _ storage: BridgeStorageLocation
    ) -> AnyView {
        AnyView(
            SettingsWindowShot(
                shown: syncing,
                page: syncSettingsPagePreview(
                    LibrarySettingsTabPreviewFixture(storage: storage),
                    shown: syncing
                )
            )
        )
    }

    /// The Sync page after setup, with no sheet.
    @MainActor
    private static func syncPage(_ storage: BridgeStorageLocation) -> ShotStage
    {
        ShotStage(syncPageWindow(storage))
    }

    /// The list of providers, with `chosen` selected.
    @MainActor
    private static func providers(_ chosen: BridgeCloudProvider) -> ShotStage {
        sheet(SyncSetupWizardStaging(step: .selectProvider, choice: chosen))
    }

    /// A provider's own screen: the folder choice or sign-in of `browser`,
    /// or iCloud's form when it has none.
    @MainActor
    private static func choosing(
        _ provider: BridgeCloudProvider,
        _ browser: StorageFolderBrowser?
    ) -> ShotStage {
        sheet(
            SyncSetupWizardStaging(
                step: .configure(provider),
                choice: provider,
                fields: browser.map { .folders($0) } ?? .iCloud
            )
        )
    }

    /// S3's form as the board draws it, with the reason `failure` gives under
    /// its field or `rejection`'s words at the top.
    @MainActor
    private static func s3Setup(
        _ failure: BridgeStorageFailure?,
        rejection: S3SetupFields.KeyRejection? = nil
    ) -> ShotStage {
        sheet(
            SyncSetupWizardStaging(
                step: .configure(.s3),
                choice: .s3,
                fields: .s3(
                    MacS3Form(
                        staged: s3Fields(failing: failure),
                        failure: rejection.map {
                            DisplayError(line: $0.words)
                        }
                    )
                )
            )
        )
    }

    /// The setup running `step`, past its point of no return when
    /// `finishing`, over the screen it started from.
    @MainActor
    private static func running(
        _ provider: BridgeCloudProvider,
        _ step: BridgeStorageSetupStep,
        finishing: Bool = false,
        browser: StorageFolderBrowser? = nil,
        s3: Bool = false
    ) -> ShotStage {
        let fields: SyncSetupWizardStaging.Provider =
            if let browser {
                .folders(browser)
            }
            else if s3 {
                .s3(MacS3Form(staged: s3Fields(failing: nil)))
            }
            else {
                .iCloud
            }
        var staging = SyncSetupWizardStaging(
            step: .configure(provider),
            choice: provider,
            fields: fields
        )
        staging.run = StorageSetupRun(
            staging: StorageSetupFollowing(
                staging: BridgeStorageSetupProgress(
                    step: step,
                    key: "core.library.storage_setup_step." + stepKey(step),
                    done: 0,
                    total: 1,
                    fraction: nil
                ),
                finishing: finishing
            )
        )
        return sheet(staging)
    }

    private static func stepKey(_ step: BridgeStorageSetupStep) -> String {
        switch step {
        case .checkingLocation: "checking_location"
        case .detectingSettings: "detecting_settings"
        case .preparingZone: "preparing_zone"
        case .readingAccount: "reading_account"
        case .connecting: "connecting"
        case .creatingStore: "creating_store"
        case .loading, .recordingAccess:
            preconditionFailure("no board shows \(step)")
        }
    }

    /// The restore code's screen, masked, over the Sync page of `connected`.
    @MainActor
    private static func restoreCode(
        _ provider: BridgeCloudProvider,
        connected: BridgeStorageLocation
    ) -> ShotStage {
        sheet(
            SyncSetupWizardStaging(
                step: .restoreCode(provider),
                choice: provider,
                reveal: RestoreCodeReveal(
                    previewState: .masked(PreviewData.restoreCodeShare)
                )
            ),
            connected: connected
        )
    }

    #if BAE_OAUTH_PROVIDERS
        /// Which screen of Dropbox or OneDrive's folder choice a board draws.
        private enum BrowserState {
            case signingIn, reading, list, naming, made, notEmpty, empty, making
        }

        @MainActor
        private static func oauthBrowser(
            _ provider: StorageFolderBrowser.Provider,
            _ state: BrowserState
        ) -> StorageFolderBrowser {
            let names =
                provider == .dropbox
                ? ["Family", "Music", "Photos"] : ["Live", "Albums", "Mixes"]
            let chosen = provider == .dropbox ? "Music" : "Albums"
            func staged(
                _ names: [String],
                selected: String? = nil,
                isNaming: Bool = false,
                newFolderName: String = "",
                notEmpty: String? = nil,
                working: StorageFolderBrowser.Work? = nil
            ) -> StorageFolderBrowser {
                let shown =
                    provider == .dropbox ? dropbox(names) : oneDrive(names)
                let named = { (name: String?) in
                    shown.folders.first { $0.name == name }
                }
                return StorageFolderBrowser(
                    staged: provider,
                    listing: StorageFolderBrowser.Listing(
                        path: shown.path,
                        folders: shown.folders
                    ),
                    nests: provider == .oneDrive,
                    selected: named(selected),
                    isNaming: isNaming,
                    newFolderName: newFolderName,
                    notEmpty: named(notEmpty),
                    working: working
                )
            }
            switch state {
            case .signingIn, .reading:
                return StorageFolderBrowser(
                    staged: provider,
                    listing: nil,
                    nests: provider == .oneDrive,
                    working: state == .signingIn ? .signingIn : .listing
                )
            case .list:
                return staged(names, selected: chosen)
            case .naming:
                return staged(names, isNaming: true, newFolderName: "bae")
            case .made:
                return staged(names + ["bae"], selected: "bae")
            case .notEmpty:
                return staged(names, selected: chosen, notEmpty: chosen)
            case .empty:
                return staged([])
            case .making:
                return staged(names, working: .makingFolder("bae"))
            }
        }
    #endif
}

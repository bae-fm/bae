import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Sync page's boards (`mac.settings.sync.*`) that show what sync
    /// waits on or why its cloud home isn't shown: sync waiting, the setup
    /// that ended undone, a cloud home that couldn't be read, and the
    /// restore code shown. Each shows the whole Settings window, as the
    /// boards draw it: the sidebar with Library chosen, and the page over the
    /// Library pane.
    static let syncSettingsScenes: [ShotScene] =
        [
            ShotScene(
                id: "mac.settings.sync.pending",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                ShotStage(
                    syncSettingsWindow(
                        LibrarySettingsTabPreviewFixture(
                            storage: PreviewData.previewDropboxStorage
                        ),
                        shown: BridgeSyncShown(
                            status: .waitingToSync,
                            statusKey: "core.sync.status.waiting_to_sync",
                            tone: .neutral,
                            cause: nil,
                            fix: nil,
                            lastSynced: Date(timeIntervalSinceNow: -2 * 3_600)
                        )
                    )
                )
            },
            ShotScene(
                id: "mac.settings.sync.setup-undone",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                ShotStage(
                    syncSettingsWindow(
                        LibrarySettingsTabPreviewFixture(storage: nil),
                        shown: .fixtureNotSynced,
                        undoneSetup: BridgeUndoneStorageSetup(
                            operation: 1,
                            error: BridgeError.Diagnostic(
                                category: .storageSetupUndone,
                                detail: "the setup was stopped partway"
                            )
                        )
                    )
                )
            },
            ShotScene(
                id: "mac.settings.sync.code-shown",
                size: CGSize(width: 690, height: 600),
                frame: .window(title: "Settings", titleBar: .hidden)
            ) {
                ShotStage(
                    syncSettingsWindow(
                        LibrarySettingsTabPreviewFixture(
                            storage: PreviewData.previewDropboxStorage
                        ),
                        shown: .fixtureSynced,
                        restoreCode: .shown(PreviewData.restoreCodeShare)
                    )
                )
            },
        ] + s3KeyScenes

    /// The Sync page of an S3 library (`mac.settings.sync.s3`,
    /// `mac.settings.sync.signed-out-s3` and
    /// `mac.settings.sync.s3-key-replaced`): its key's id with Replace key…,
    /// and the line after a saved key. Then the S3 key sheet over it
    /// (`mac.settings.sync.s3-replace-key-*` and
    /// `mac.settings.sync.signed-out-s3-sign-in`): empty, typed, checking
    /// the key, and refused.
    private static let s3KeyScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.sync.s3",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Page(key: "AKIA4EXAMPLEOLD")
        },
        ShotScene(
            id: "mac.settings.sync.signed-out-s3",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Page(key: "AKIA4EXAMPLEOLD", shown: signedOutOfS3)
        },
        ShotScene(
            id: "mac.settings.sync.s3-key-replaced",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3Page(
                key: "AKIA4EXAMPLENEW",
                keyReplacement: S3KeyReplacement(
                    saved: .init(oldKey: "AKIA4EXAMPLEOLD")
                )
            )
        },
        ShotScene(
            id: "mac.settings.sync.s3-replace-key-empty",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3SheetPage(.fields(nil))
        },
        ShotScene(
            id: "mac.settings.sync.s3-replace-key-filled",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3SheetPage(.fields(nil), typed: typedKey)
        },
        ShotScene(
            id: "mac.settings.sync.s3-replace-key-checking",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3SheetPage(.checking, typed: typedKey)
        },
        ShotScene(
            id: "mac.settings.sync.s3-replace-key-refused",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3SheetPage(.fields(.refused), typed: typedKey)
        },
        ShotScene(
            id: "mac.settings.sync.signed-out-s3-sign-in",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            s3SheetPage(.fields(nil), shown: signedOutOfS3)
        },
    ]

    /// The new key the boards' sheets show typed.
    private static let typedKey = (
        accessKey: "AKIA4EXAMPLENEW", secretKey: "abcdefghijkl"
    )

    /// Signed out of S3, three days after the last sync.
    private static let signedOutOfS3 = BridgeSyncShown(
        status: .signedOutOfS3,
        statusKey: "core.sync.status.signed_out_s3",
        tone: .warning,
        cause: .changesStayOnDevice,
        fix: .enterAccessKey,
        lastSynced: Date(timeIntervalSinceNow: -3 * 86_400)
    )

    /// The S3 storage the boards draw.
    private static let boardS3Storage = BridgeStorageLocation.s3(
        bucket: "my-bucket",
        region: "us-east-1",
        endpoint: "https://s3.home.lan",
        profile: nil
    )

    /// The Sync page of the boards' S3 library, which records `key` for the
    /// person.
    @MainActor
    private static func s3Window(
        key: String,
        shown: BridgeSyncShown,
        keyReplacement: S3KeyReplacement
    ) -> some View {
        syncSettingsWindow(
            LibrarySettingsTabPreviewFixture(
                storage: boardS3Storage,
                storageAccess: .s3AccessKey(accessKeyId: key)
            ),
            shown: shown,
            keyReplacement: keyReplacement
        )
    }

    /// The page of the boards' S3 library, which records its old key, with
    /// the S3 key sheet open over it as `sheet` and `typed` show it. The
    /// page's own replacement stays closed: the scene lays the sheet over
    /// the window, as `OverWindow` says.
    @MainActor
    private static func s3SheetPage(
        _ sheet: S3KeyReplacement.Sheet,
        shown: BridgeSyncShown = .fixtureSynced,
        typed: (accessKey: String, secretKey: String) = ("", "")
    ) -> ShotStage {
        ShotStage(
            OverWindow.sheet(
                S3KeySheet(
                    replacement: S3KeyReplacement(sheet: sheet),
                    accessKey: typed.accessKey,
                    secretKey: typed.secretKey
                )
                .environment(PreviewData.previewSync()),
                over: s3Window(
                    key: "AKIA4EXAMPLEOLD",
                    shown: shown,
                    keyReplacement: S3KeyReplacement()
                )
            )
        )
    }

    /// The page of the boards' S3 library, which records `key` for the
    /// person.
    @MainActor
    private static func s3Page(
        key: String,
        shown: BridgeSyncShown = .fixtureSynced,
        keyReplacement: S3KeyReplacement = S3KeyReplacement()
    ) -> ShotStage {
        ShotStage(
            s3Window(key: key, shown: shown, keyReplacement: keyReplacement)
        )
    }
}

/// The Settings window around the Sync page of `fixture`'s library: the
/// sidebar as `SettingsView` lists it, and the page `syncSettingsPagePreview`
/// builds beside it.
@MainActor
private func syncSettingsWindow(
    _ fixture: LibrarySettingsTabPreviewFixture,
    shown: BridgeSyncShown,
    undoneSetup: BridgeUndoneStorageSetup? = nil,
    restoreCode: RestoreCodeReveal.State = .masked(
        PreviewData.restoreCodeShare
    ),
    keyReplacement: S3KeyReplacement = S3KeyReplacement()
) -> some View {
    SettingsWindowShot(
        shown: shown,
        undoneSetup: undoneSetup,
        page: syncSettingsPagePreview(
            fixture,
            shown: shown,
            undoneSetup: undoneSetup,
            restoreCode: restoreCode,
            keyReplacement: keyReplacement
        )
    )
}

/// The Settings window on the Library pane, showing People and devices
/// `subpage` for an admin with `invites` open, with `alert` laid over it.
@MainActor
func peopleSettingsWindow(
    subpage: PeopleAndDevicesPage.Subpage,
    invites: [BridgeInvite] = [],
    alert: (MacAlertWords, [MacAlertButton])
) -> some View {
    let membership = MembershipStore.stub(
        people: PreviewData.members(
            reaching: PreviewData.previewDropboxStorage
        ),
        selfIsAdmin: true,
        invites: invites
    )
    return OverWindow.alert(
        alert,
        over: SettingsWindowShot(
            shown: .fixtureSynced,
            undoneSetup: nil,
            page: PeopleAndDevicesPage(onBack: {}, subpage: subpage)
                .environment(
                    PreviewData.previewSync()
                )
                .environment(
                    PreviewData.connectedConfigStore(
                        storage: PreviewData.previewDropboxStorage
                    )
                )
                .environment(membership)
        )
    )
}

import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac's alerts, each drawn over the window the app opens it on: the
    /// album and error alerts over the library window, the pane alerts over
    /// Settings, and the unpin confirmation over the Storage manager.
    static let alertScenes: [ShotScene] =
        [
            ShotScene(
                id: "mac.album.delete-album",
                size: CGSize(width: 1_200, height: 1_000),
                frame: .window(title: "bae", titleBar: .hidden)
            ) {
                libraryAlertStage(
                    DeleteAlbumConfirmation.alert {},
                    height: 1_000,
                    openAlbum: true
                )
            },
            ShotScene(
                id: "mac.album.delete-release",
                size: CGSize(width: 1_200, height: 1_000),
                frame: .window(title: "bae", titleBar: .hidden)
            ) {
                libraryAlertStage(
                    DeleteReleaseConfirmation.alert {},
                    height: 1_000,
                    openAlbum: true
                )
            },
            ShotScene(
                id: "mac.album.storage.unpin",
                size: CGSize(width: 1_200, height: 1_000),
                frame: .window(title: "bae", titleBar: .hidden)
            ) {
                libraryAlertStage(
                    UnpinConfirmation.alert(
                        for: PendingUnpin(
                            releaseIds: ["release"],
                            title: "Low Tide Hours"
                        ),
                        dontShowAgain: .constant(false)
                    ) { _ in },
                    height: 1_000,
                    openAlbum: true
                )
            },
            unpinSeveral(),
            ShotScene(
                id: "mac.errors.alert",
                size: CGSize(width: 1_200, height: 760),
                frame: .window(title: "bae", titleBar: .hidden)
            ) {
                libraryAlertStage(
                    ErrorAlertContent.alert(
                        title: Text("Error"),
                        error: DisplayError(
                            line: "Could not reach the cloud.",
                            detail: "dns error: failed to lookup address"
                        )
                    ),
                    height: 760
                )
            },
            ShotScene(
                id: "mac.errors.alert.play-outcome",
                size: CGSize(width: 1_200, height: 760),
                frame: .window(title: "bae", titleBar: .hidden)
            ) {
                libraryAlertStage(
                    ErrorAlertContent.alert(
                        title: Text("Error"),
                        error: DisplayError(
                            line: "There are no tracks to play."
                        )
                    ),
                    height: 760
                )
            },
        ] + settingsPaneAlertScenes + devicesAlertScenes + overWindowAlertScenes

    /// The alerts the Import and Save as panes raise, over the Settings
    /// window on that pane.
    private static let settingsPaneAlertScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.import.turn-off-discogs",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                OverWindow.alert(
                    TurnOffDiscogsConfirmation.alert {},
                    over: SettingsWindowShot(
                        tab: .importing,
                        page: ImportSettingsTab()
                            .environment(
                                PreviewData.makeConfigStore(
                                    libraryFullWidth: false,
                                    importWhenIdentified: true,
                                    storage: PreviewData.previewStorage
                                )
                            )
                            .environment(Discogs.stub())
                            .environment(Importer.stub())
                            .environment(UiStore())
                    )
                )
            )
        },
        ShotScene(
            id: "mac.settings.save-as.delete-format",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                OverWindow.alert(
                    DeleteFormatConfirmation.alert(named: "MP3") {},
                    over: SettingsWindowShot(
                        tab: .formats,
                        page: FormatsSettingsTab()
                            .environment(PreviewData.configStore())
                            .environment(Outputs.stub())
                            .environment(UiStore())
                    )
                )
            )
        },
    ]

    /// The alerts People and devices raises, over the Settings window.
    private static let devicesAlertScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.devices.cut-off",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                peopleSettingsWindow(
                    subpage: .devices,
                    alert: CutOffAlerts.step(
                        .removeAppAccess(provider: .dropbox),
                        offersCutOff: true
                    ) {}
                )
            )
        },
        ShotScene(
            id: "mac.settings.devices.cut-off-other",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                peopleSettingsWindow(
                    subpage: .devices,
                    alert: CutOffAlerts.step(
                        .removeAppAccess(provider: .dropbox),
                        offersCutOff: false
                    ) {}
                )
            )
        },
        ShotScene(
            id: "mac.settings.devices.cut-off-running",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                peopleSettingsWindow(
                    subpage: .devices,
                    alert: CutOffAlerts.retry(.providerRequestRunning) {}
                )
            )
        },
        ShotScene(
            id: "mac.settings.devices.cut-off-not-here",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                peopleSettingsWindow(
                    subpage: .devices,
                    alert: CutOffAlerts.retry(.deviceNotGone) {}
                )
            )
        },
        ShotScene(
            id: "mac.settings.invite.cancel",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let invite = BridgeInvite(
                id: "11",
                member: "4a1f2c3d-0000-4000-8000-000000000009",
                label: "dan@example.com",
                labelInitials: "D",
                access: .providerAccount(email: "dan@example.com"),
                shown: .code(code: "invite-code", progress: .waiting),
                held: nil
            )
            return ShotStage(
                peopleSettingsWindow(
                    subpage: .people,
                    invites: [invite],
                    alert: InviteCancelAlert.alert(for: invite) {}
                )
            )
        },
    ]

    /// The alerts over a window that is not Settings: resetting a device that
    /// must be reset, and the details of a library that could not be read and
    /// of a problem the Activity panel lists.
    private static let overWindowAlertScenes: [ShotScene] = [
        ShotScene(
            id: "mac.welcome.reset.confirm",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                OverWindow.alert(
                    DeviceResetConfirmation.alert {},
                    over: ShotScenes.deviceReset(error: nil)
                )
            )
        },
        ShotScene(
            id: "mac.library.view-failed",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let uiStore = UiStore()
            let error = DisplayError(
                line: "Could not read your library",
                detail: LibraryView.unreadableDetail
            )
            let backing = LibraryView.previewUnreadableBacking(uiStore: uiStore)
            return ShotStage(
                OverWindow.alert(
                    ErrorDetails(error: error)?.alert
                        ?? ErrorAlertContent.alert(
                            title: Text("Error"),
                            error: error
                        ),
                    over: ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        needsAttention: false,
                        playbackStore: boardPlayback(),
                        size: CGSize(width: 1200, height: 760)
                    )
                )
            ) {
                backing.session.albums.list?.initialLoadError != nil
            }
        },
        ShotScene(
            id: "mac.library.albums.failed",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            let backing = LibraryView.previewUnreadableBacking(uiStore: uiStore)
            return ShotStage(
                ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    needsAttention: false,
                    size: CGSize(width: 1200, height: 760)
                )
            ) {
                backing.session.albums.list?.initialLoadError != nil
            }
        },
        ShotScene(
            id: "mac.activity.panel.problem-open",
            size: CGSize(width: 1200, height: 760),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let uiStore = UiStore()
            let problem = LaunchProblems(
                signInSettings: DisplayError(
                    line: "The sign-in settings file is missing.",
                    detail: "OAuthConfig: missing client id"
                )
            )
            .problems[0]
            let backing = boardLibraryBacking(uiStore: uiStore)
            return ShotStage(
                OverWindow.alert(
                    ErrorDetails(
                        title: problem.title,
                        advice: problem.advice,
                        error: problem.error
                    )
                    .alert,
                    over: ShotScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        playbackStore: boardPlayback(),
                        size: CGSize(width: 1200, height: 760)
                    )
                )
            ) {
                (backing.session.albums.list?.positionCount ?? 0) > 0
            }
        },
    ]

    /// An alert raised over the library window as the app raises it, 1200
    /// points wide and `height` tall as the boards draw it.
    private static func libraryAlertStage(
        _ alert: (MacAlertWords, [MacAlertButton]),
        height: CGFloat,
        openAlbum: Bool = false
    ) -> ShotStage {
        let uiStore = UiStore()
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            openAlbum: openAlbum
        )
        return ShotStage(
            OverWindow.alert(
                alert,
                over: ShotScenes.libraryWindow(
                    uiStore: uiStore,
                    backing: backing,
                    syncActivity: SyncActivityStore(),
                    playbackStore: boardPlayback(),
                    size: CGSize(width: 1_200, height: height)
                )
            )
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }

    /// The confirmation to unpin the three pinned releases the board selects
    /// in its twelve, over the Storage manager in its window with the
    /// inspector open.
    private static func unpinSeveral() -> ShotScene {
        ShotScene(
            id: "mac.storage.unpin-several",
            size: storageManagerSize,
            frame: .window(title: "Storage Manager")
        ) {
            let pinned: Set<String> = ["rel-row-3", "rel-row-10", "rel-row-12"]
            // Core's answer for the three: core returns no storage actions
            // for these rows, none of them is being transferred and no move
            // has failed, and core names no single release because three are
            // selected, so the inspector shows Select a release.
            let selected = boardStorageRows.filter { pinned.contains($0.id) }
            let fixture = StorageManagerPreviewFixture(
                rows: boardStorageRows,
                inspectorPresented: true,
                downloads: [],
                outputs: [],
                uploads: [],
                selections: [
                    pinned: BridgeStorageSelection(
                        asked: pinned.sorted(),
                        selected: selected,
                        sole: nil,
                        selectionActions: [],
                        cancelOnly: false
                    )
                ],
                queues: boardStorageQueues(pinning: "rel-row-6")
            )
            let streams = BoardTransferStreams(pinning: "rel-row-6")
            let manager =
                fixture.environment(
                    of: MacStorageWindow(
                        initialSelection: pinned,
                        initialInspectorPresented: fixture
                            .initialInspectorPresented
                    ),
                    downloads: streams.downloads,
                    outputs: streams.outputs,
                    outbox: streams.outbox
                )
                .frame(
                    width: storageManagerSize.width,
                    height: storageManagerSize.height
                )
            return ShotStage(
                OverWindow.alert(
                    UnpinConfirmation.alert(
                        for: PendingUnpin(
                            releaseIds: pinned.sorted(),
                            title: nil
                        ),
                        dontShowAgain: .constant(false)
                    ) { _ in },
                    over: manager
                )
            ) {
                fixture.storageManagerStore.list != nil
            }
        }
    }
}

extension LibraryView {
    /// What the library window's failure says below its line, as the board
    /// words it.
    nonisolated static let unreadableDetail =
        "SqliteFailure(DatabaseCorrupt)\ndatabase disk image is malformed"

    /// A library whose album read fails in a way the board offers Retry
    /// for, with a started `LibraryBrowseSession` that reads it, so the
    /// albums pane shows the notice for a list that the session could not
    /// read.
    @MainActor
    static func previewUnreadableBacking(
        uiStore: UiStore
    ) -> (library: Library, session: LibraryBrowseSession) {
        let library = Library(
            albumList: { _, _ in
                .failing(
                    BridgeError.Diagnostic(
                        category: .databaseBusy,
                        detail: LibraryView.unreadableDetail
                    ),
                    answers: UnansweredCount()
                )
            },
            playlists: noPlaylists
        )
        let session = LibraryBrowseSession(
            library: library,
            uiStore: uiStore
        )
        session.start()
        return (library, session)
    }
}

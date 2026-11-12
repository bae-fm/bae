import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac's alerts, each over the window it opens on: the album and
    /// error alerts over the library window, the pane alerts over Settings,
    /// and the unpin confirmation of the Storage manager, which no scene
    /// stages a window for yet, as bae's alert face alone.
    static let alertScenes: [ShotScene] =
        [
            overLibrary(
                "mac.album.delete-album",
                DeleteAlbumConfirmation.alert {},
                height: 1_000
            ),
            overLibrary(
                "mac.album.delete-release",
                DeleteReleaseConfirmation.alert {},
                height: 1_000
            ),
            overLibrary(
                "mac.album.storage.unpin",
                UnpinConfirmation.alert(
                    for: PendingUnpin(
                        releaseIds: ["release"],
                        title: "Low Tide Hours"
                    ),
                    dontShowAgain: .constant(false)
                ) { _ in },
                height: 1_000
            ),
            alert(
                "mac.storage.unpin-several",
                UnpinConfirmation.alert(
                    for: PendingUnpin(
                        releaseIds: ["a", "b", "c"],
                        title: nil
                    ),
                    dontShowAgain: .constant(false)
                ) { _ in },
                height: 220
            ),
            overLibrary(
                "mac.errors.alert",
                ErrorAlertContent.alert(
                    title: Text("Error"),
                    error: DisplayError(
                        line: "Could not reach the cloud.",
                        detail: "dns error: failed to lookup address"
                    )
                ),
                height: 760
            ),
            overLibrary(
                "mac.errors.alert.play-outcome",
                ErrorAlertContent.alert(
                    title: Text("Error"),
                    error: DisplayError(line: "There are no tracks to play.")
                ),
                height: 760
            ),
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
                            .environment(PreviewData.configStore())
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
            size: WelcomeWindow.size
        ) {
            ShotStage(
                OverWindow.alert(
                    DeviceResetConfirmation.alert {},
                    over: PreviewScenes.deviceReset(
                        error: LoadError(
                            .resetOffered,
                            PreviewData.resetRequiredError
                        )
                    )
                )
            )
        },
        ShotScene(
            id: "mac.library.view-failed",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            let error = DisplayError(
                line: "Could not read your library",
                detail: "database is locked"
            )
            let backing = LibraryView.previewUnreadableBacking(uiStore: uiStore)
            return ShotStage(
                OverWindow.alert(
                    ErrorDetails(error: error)?.alert
                        ?? ErrorAlertContent.alert(
                            title: Text("Error"),
                            error: error
                        ),
                    over: PreviewScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        needsAttention: false,
                        size: CGSize(width: 1200, height: 760)
                    )
                )
            ) {
                backing.session.albums.list?.initialLoadError != nil
            }
        },
        ShotScene(
            id: "mac.activity.panel.problem-open",
            size: CGSize(width: 1200, height: 760)
        ) {
            let uiStore = UiStore()
            let problem = LaunchProblems(
                signInSettings: DisplayError(
                    line: "The sign-in settings file is missing.",
                    detail: "OAuthConfig: missing client id"
                )
            )
            .problems[0]
            let backing = LibraryView.previewGridBacking(uiStore: uiStore)
            return ShotStage(
                OverWindow.alert(
                    ErrorDetails(
                        title: problem.title,
                        advice: problem.advice,
                        error: problem.error
                    )
                    .alert,
                    over: PreviewScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        size: CGSize(width: 1200, height: 760)
                    )
                )
            ) {
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        },
    ]

    /// An alert hung over the library window as the app shows it, 1200
    /// points wide and `height` tall as the boards draw it.
    private static func overLibrary(
        _ id: String,
        _ alert: (MacAlertWords, [MacAlertButton]),
        height: CGFloat
    ) -> ShotScene {
        let size = CGSize(width: 1_200, height: height)
        return ShotScene(id: id, size: size) {
            let uiStore = UiStore()
            let backing = LibraryView.previewGridBacking(uiStore: uiStore)
            return ShotStage(
                OverWindow.alert(
                    alert,
                    over: PreviewScenes.libraryWindow(
                        uiStore: uiStore,
                        backing: backing,
                        size: size
                    )
                )
            ) {
                (backing.session.albums.list?.totalCount ?? 0) > 0
            }
        }
    }

    private static func alert(
        _ id: String,
        _ alert: (MacAlertWords, [MacAlertButton]),
        height: CGFloat
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 340, height: height)) {
            ShotStage(AlertShot(alert: alert))
        }
    }
}

extension LibraryView {
    /// `previewEmptyBacking` for a library whose database is locked by
    /// another process: the album read fails in a way asking again can
    /// get past, so the albums pane shows the notice for a list that could
    /// not be read, with Retry.
    @MainActor
    static func previewUnreadableBacking(
        uiStore: UiStore
    ) -> (library: Library, session: LibraryBrowseSession) {
        let library = Library(
            albumBrowse: { _, _ in
                .failing(
                    BridgeError.Diagnostic(
                        category: .databaseBusy,
                        detail: "database is locked"
                    ),
                    answers: UnansweredCount()
                )
            }
        )
        let session = LibraryBrowseSession(
            library: library,
            uiStore: uiStore
        )
        session.start()
        return (library, session)
    }
}

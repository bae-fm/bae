import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac's alerts that ask or report without a window's state behind
    /// them (`mac.errors.alert*`, the delete and unpin confirmations): bae's
    /// alert face alone.
    static let alertScenes: [ShotScene] =
        [
            alert(
                "mac.album.delete-album",
                DeleteAlbumConfirmation.alert {},
                height: 170
            ),
            alert(
                "mac.album.delete-release",
                DeleteReleaseConfirmation.alert {},
                height: 170
            ),
            alert(
                "mac.album.storage.unpin",
                UnpinConfirmation.alert(
                    for: PendingUnpin(
                        releaseIds: ["release"],
                        title: "Low Tide Hours"
                    ),
                    dontShowAgain: .constant(false)
                ) { _ in },
                height: 220
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
            alert(
                "mac.settings.import.turn-off-discogs",
                TurnOffDiscogsConfirmation.alert {},
                height: 140
            ),
            alert(
                "mac.settings.save-as.delete-format",
                DeleteFormatConfirmation.alert(named: "MP3") {},
                height: 130
            ),
            alert(
                "mac.errors.alert",
                ErrorAlertContent.alert(
                    title: Text("Error"),
                    error: DisplayError(
                        line: "Could not reach the cloud.",
                        detail: "dns error: failed to lookup address"
                    )
                ),
                height: 200
            ),
            alert(
                "mac.errors.alert.play-outcome",
                ErrorAlertContent.alert(
                    title: Text("Error"),
                    error: DisplayError(line: "There are no tracks to play.")
                ),
                height: 130
            ),
        ] + devicesAlertScenes + overWindowAlertScenes

    /// The alerts People and devices raises, over the Settings window.
    private static let devicesAlertScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.devices.cut-off",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
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
            frame: .window(title: "Settings")
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

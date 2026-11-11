import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The welcome boards (`mac.welcome.*`): the welcome window over this
    /// Mac's libraries or none, a library's details, the alerts of removing
    /// one, the screens a library opens or resets behind, and the code sheets.
    /// Not shot: the library menu and the Add library menu (system menus),
    /// the reset confirmation (a SwiftUI alert over the screen), the scanner
    /// (the camera), and Add this device and Join, which aren't built.
    static let welcomeScenes: [ShotScene] = [
        welcomeWindow(
            "mac.welcome.libraries",
            libraries: PreviewData.boardLibraries
        ),
        welcomeWindow("mac.welcome.libraries.none", libraries: []),
        detailsScene("mac.welcome.library-details", PreviewData.standingSynced),
        detailsScene(
            "mac.welcome.library-details.unsent-changes",
            PreviewData.standingUnsent
        ),
        removalAlert(
            "mac.welcome.delete-never-synced",
            PreviewData.odilesRecords
        ),
        removalAlert("mac.welcome.remove", PreviewData.homeMusic),
        removalAlert(
            "mac.welcome.remove.unsent-changes",
            PreviewData.homeMusic,
            losing: .counted(holdsUnsent: true)
        ),
        ShotScene(id: "mac.welcome.remove.removing", size: WelcomeWindow.size) {
            let reads = CountedReads(libraries: PreviewData.boardLibraries)
            let removals = PreviewData.welcomeRemovals(
                stoppedAt: .leavingLibrary
            )
            removals.remove(
                closedLibrary: PreviewData.homeMusic,
                losing: .counted(holdsUnsent: false),
                onFinished: {}
            )
            return ShotStage(
                PreviewScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup,
                    removals: removals
                )
            ) { reads.answered == 2 }
        },
        ShotScene(
            id: "mac.welcome.remove.failed",
            size: CGSize(width: 340, height: 160)
        ) {
            ShotStage(
                NSAlertShot {
                    LibraryRemovalAlerts.failure(
                        FailedRemoval(
                            library: PreviewData.homeMusic,
                            error: DisplayError(
                                line:
                                    "Couldn’t reach the cloud. Check your connection and try again."
                            ),
                            retry: .ask
                        )
                    )
                }
            )
        },
        ShotScene(
            id: "mac.welcome.opening.finding-libraries",
            size: WelcomeWindow.size
        ) {
            ShotStage(
                WelcomeWindowChrome {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.findingLibraries.title,
                        leading: nil,
                        opening: nil,
                        reset: nil
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.opening.steps", size: WelcomeWindow.size) {
            ShotStage(
                WelcomeWindowChrome {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.opening.title,
                        leading: nil,
                        opening:
                            LatestStream.openProgress(
                                holding: BridgeOpenProgress(
                                    step: .rebuilding,
                                    done: 45,
                                    total: 100
                                )
                            )
                            .only,
                        reset: nil
                    )
                }
            )
        },
        ShotScene(
            id: "mac.welcome.opening.library-locked",
            size: WelcomeWindow.size
        ) {
            ShotStage(WelcomeWindowChrome { KeychainLockedView(onRetry: {}) })
        },
        ShotScene(id: "mac.welcome.reset.offered", size: WelcomeWindow.size) {
            ShotStage(
                PreviewScenes.deviceReset(
                    error: LoadError(
                        .resetOffered,
                        PreviewData.resetRequiredError
                    )
                )
            )
        },
        ShotScene(id: "mac.welcome.opening.resetting", size: WelcomeWindow.size)
        {
            let progress = BridgeLoadProgress(
                step: .streamingLibrary,
                done: 5,
                total: 10
            )
            return ShotStage(
                WelcomeWindowChrome {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.resetting.title,
                        leading: nil,
                        opening: nil,
                        reset: MacWelcomeOpeningReset(
                            steps:
                                LatestStream.bootstrapSteps(
                                    holding: .loading(progress: progress)
                                )
                                .only,
                            status: .loading
                        )
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.reset.failed", size: WelcomeWindow.size) {
            ShotStage(
                PreviewScenes.deviceReset(
                    error: LoadError(
                        .deviceReset,
                        PreviewData.displayErrorWithDetail
                    )
                )
            )
        },
        ShotScene(id: "mac.welcome.reset.unfinished", size: WelcomeWindow.size)
        {
            ShotStage(
                WelcomeWindowChrome {
                    DeviceResetView(
                        prompt: .unfinished,
                        error: nil,
                        onReset: {},
                        onCancel: {}
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.create-name", size: WelcomeWindow.size) {
            ShotStage(
                PreviewScenes.welcome(
                    initialMode: .create,
                    setup: PreviewData.welcomeSetup()
                )
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.entering",
            size: CGSize(width: 400, height: 190)
        ) {
            ShotStage(
                EnterCodeSheet(form: enterCodeForm(), onCancel: {})
                    .windowBackground()
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.half-typed",
            size: CGSize(width: 400, height: 190)
        ) {
            let form = enterCodeForm()
            form.edit(code: PreviewData.restoreCode)
            return ShotStage(
                EnterCodeSheet(form: form, onCancel: {})
                    .windowBackground()
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.s3-key-needed",
            size: CGSize(width: 440, height: 210)
        ) {
            let form = enterCodeForm(reading: .restore(metadata: s3Metadata))
            form.acceptCode(PreviewData.restoreCode)
            form.accessKey = "AKIA4EXAMPLE"
            return ShotStage(RestoreS3KeySheet(form: form).windowBackground())
        },
        ShotScene(id: "mac.welcome.add-code.adding", size: WelcomeWindow.size) {
            ShotStage(
                WelcomeWindowChrome {
                    CodeRunningView(
                        openSteps: nil,
                        steps:
                            LatestStream.bootstrapSteps(
                                holding: .loading(
                                    progress: BridgeLoadProgress(
                                        step: .streamingLibrary,
                                        done: 3,
                                        total: 10
                                    )
                                )
                            )
                            .only,
                        status: .loading,
                        onCancel: {}
                    )
                }
            )
        },
    ]

    /// The welcome window over `libraries`, on the first screen.
    private static func welcomeWindow(
        _ id: String,
        libraries: [BridgeLibrary]
    ) -> ShotScene {
        ShotScene(id: id, size: WelcomeWindow.size) {
            let reads = CountedReads(libraries: libraries)
            return ShotStage(
                PreviewScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup
                )
            ) { reads.answered == 2 }
        }
    }

    /// The Enter code sheet's state: the code is read as `info` when the
    /// person chooses Continue, and nothing it starts reaches a library.
    private static func enterCodeForm(
        reading info: BridgeCodeInfo? = nil
    ) -> BootstrapCodeForm {
        BootstrapCodeForm(
            entry: .code,
            decodeCode: { _ in
                guard let info else { throw UnservedReadError() }
                return info
            },
            makeBootstrap: {
                LibraryBootstrap(
                    host: BaeHost.preview(),
                    deviceName: "Preview Device"
                )
            },
            unfinishedJoins: { [] },
            unfinishedRestores: { [] },
            onLibraryReady: { _, _ in }
        )
    }

    /// A restore code for an S3 library whose key the code doesn't hold.
    private static let s3Metadata = BridgeCodeMetadata(
        libraryId: "library-preview",
        memberId: "member-preview",
        provider: .s3,
        location: .s3(
            bucket: "music-library",
            region: "us-east-1",
            endpoint: nil,
            profile: nil
        ),
        oauthAccess: nil,
        carriesS3Key: false
    )

    /// The confirmation `library` asks before it is removed, counted as
    /// `loss`.
    private static func removalAlert(
        _ id: String,
        _ library: BridgeLibrary,
        losing loss: RemovalLoss = .counted(holdsUnsent: false)
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 340, height: 280)) {
            ShotStage(
                NSAlertShot {
                    LibraryRemovalAlerts.confirmation(
                        for: library,
                        losing: loss
                    )
                }
            )
        }
    }

    /// The details popover's content for Home music, once its status
    /// is in.
    private static func detailsScene(
        _ id: String,
        _ status: BridgeLibraryStanding
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 320, height: 380)) {
            let reads = CountedReads()
            return ShotStage(
                PreviewScenes.welcomeLibraryDetails(
                    PreviewData.homeMusic,
                    readStanding: reads.readStanding(status)
                )
            ) { reads.answered == 1 }
        }
    }
}

private struct UnservedReadError: Error {}

/// Lookups a scene's view makes on appear, counted as they are answered: the
/// view is ready once they are in.
@MainActor
@Observable
private final class CountedReads {
    private(set) var answered = 0
    /// The libraries on this Mac the listing answers.
    let libraries: [BridgeLibrary]

    init(libraries: [BridgeLibrary] = []) {
        self.libraries = libraries
    }

    var setup: LibrarySetup {
        let base = LibrarySetup.stub()
        return LibrarySetup(
            listLibraries: { [self] in
                await answer()
                return libraries
            },
            libraryStanding: { _ in throw UnservedReadError() },
            createLibrary: { _, _ in throw UnservedReadError() },
            savedRestoreCodes: { [self] in
                await answer()
                return []
            },
            forgetSavedRestoreCode: { _ in throw UnservedReadError() },
            unfinishedJoins: { [] },
            unfinishedRestores: { [] },
            makeBootstrap: {
                LibraryBootstrap(
                    host: BaeHost.preview(),
                    deviceName: "Preview Device"
                )
            },
            makeSetupSteps: { base.setupSteps() }
        )
    }

    /// A read of a closed library's status that answers `status`.
    func readStanding(_ status: BridgeLibraryStanding)
        -> @Sendable (String) async -> BridgeLibraryStanding
    {
        { [self] _ in
            await answer()
            return status
        }
    }

    private func answer() {
        answered += 1
    }
}

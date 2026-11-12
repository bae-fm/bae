import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The welcome boards (`mac.welcome.*`): the welcome window over this
    /// Mac's libraries or none, a library's details, the alerts of removing
    /// one, the screens a library opens or resets behind, and the code sheets.
    /// Not shot: the library menu and the Add library menu (system menus),
    /// the reset confirmation, the scanner
    /// (the camera), and Add this device, which isn't built.
    static let welcomeScenes: [ShotScene] = [
        welcomeWindow(
            "mac.welcome.libraries",
            libraries: PreviewData.boardLibraries
        ),
        welcomeWindow("mac.welcome.libraries.none", libraries: []),
        welcomeWindow(
            "mac.welcome.open-failed",
            libraries: PreviewData.boardLibraries,
            loadError: LoadError(
                .libraryOpen,
                PreviewData.displayErrorWithDetail,
                libraryId: PreviewData.homeMusic.id
            )
        ),
        welcomeWindow(
            "mac.welcome.libraries.saved-codes",
            libraries: PreviewData.boardLibraries,
            savedCodes: savedCodes
        ),
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
                stoppedAt: BridgeClosingProgress(
                    step: .leavingLibrary,
                    key: "core.library.closing_step.leaving_library"
                )
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
        overWelcome("mac.welcome.remove.failed") { _, window in
            OverWindow.alert(
                LibraryRemovalAlerts.failure(
                    FailedRemoval(
                        library: PreviewData.homeMusic,
                        error: DisplayError(
                            line:
                                "Could not reach the cloud. Check your connection and try again."
                        ),
                        retry: .ask
                    )
                ),
                over: window
            )
        },
        savedCodeDeletionAlert(),
        ShotScene(
            id: "mac.welcome.opening.finding-libraries",
            size: WelcomeWindow.size
        ) {
            ShotStage(
                MacWelcomeWindow {
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
                MacWelcomeWindow {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.opening.title,
                        leading: nil,
                        opening:
                            LatestStream.openProgress(
                                holding: BridgeOpenProgress(
                                    step: .rebuilding,
                                    key: "core.library.open_step.rebuilding",
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
            ShotStage(MacWelcomeWindow { KeychainLockedView(onRetry: {}) })
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
                key: "core.library.load_step.streaming_library",
                done: 5,
                total: 10
            )
            return ShotStage(
                MacWelcomeWindow {
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
                        PreviewData.displayErrorRetryable
                    )
                )
            )
        },
        ShotScene(id: "mac.welcome.reset.unfinished", size: WelcomeWindow.size)
        {
            ShotStage(
                MacWelcomeWindow {
                    DeviceResetView(
                        prompt: .unfinished,
                        error: nil,
                        onReset: {},
                        onCancel: {}
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.create.name", size: WelcomeWindow.size) {
            ShotStage(
                PreviewScenes.welcome(
                    initialMode: .create,
                    setup: PreviewData.welcomeSetup()
                )
            )
        },
        ShotScene(id: "mac.welcome.join.name", size: WelcomeWindow.size) {
            ShotStage(
                MacWelcomeWindow {
                    JoinNameView(
                        libraryName: "Home music",
                        name: .constant(""),
                        error: nil,
                        canJoin: false,
                        onJoin: {},
                        onCancel: {}
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.join.waiting", size: WelcomeWindow.size) {
            ShotStage(
                MacWelcomeWindow {
                    JoinWaitingView(
                        invite: BridgeInviteInfo(
                            metadata: BridgeCodeMetadata(
                                libraryId: "home",
                                memberId: "ana",
                                provider: .s3,
                                location: .s3(
                                    bucket: "music-library",
                                    region: "us-east-1",
                                    endpoint: nil,
                                    profile: nil
                                ),
                                oauthAccess: nil,
                                carriesS3Key: false
                            ),
                            libraryName: "Home music",
                            inviter: "Ana"
                        ),
                        status: .waitingForApproval(confirmation: "412 778"),
                        steps: nil,
                        error: nil,
                        cancellable: false,
                        onCancel: {}
                    )
                }
            )
        },
        ShotScene(id: "mac.welcome.join.ended", size: WelcomeWindow.size) {
            ShotStage(
                MacWelcomeWindow {
                    JoinEndedView(inviter: "Ana", onOK: {})
                }
            )
        },
        overWelcome("mac.welcome.add-code.entering", libraries: []) {
            _,
            window in
            OverWindow.sheet(
                EnterCodeSheet(form: enterCodeForm(), onCancel: {})
                    .frame(width: 400)
                    .fixedSize(horizontal: false, vertical: true),
                over: window
            )
        },
        overWelcome("mac.welcome.add-code.scanning", libraries: []) {
            _,
            window in
            OverWindow.sheet(
                CodeScannerSheet(
                    onScan: { _ in },
                    onDismiss: {},
                    startsCamera: false
                )
                .frame(width: 400)
                .fixedSize(horizontal: false, vertical: true),
                over: window
            )
        },
        overWelcome("mac.welcome.add-code.half-typed", libraries: []) {
            _,
            window in
            let form = enterCodeForm()
            form.edit(code: String(PreviewData.restoreCode.prefix(25)))
            return OverWindow.sheet(
                EnterCodeSheet(form: form, onCancel: {})
                    .frame(width: 400)
                    .fixedSize(horizontal: false, vertical: true),
                over: window
            )
        },
        overWelcome("mac.welcome.add-code.s3-key-needed", libraries: []) {
            _,
            window in
            let form = enterCodeForm(reading: .restore(metadata: s3Metadata))
            form.acceptCode(PreviewData.restoreCode)
            form.accessKey = "AKIA4EXAMPLE"
            return OverWindow.sheet(
                RestoreS3KeySheet(form: form)
                    .frame(width: 440)
                    .fixedSize(horizontal: false, vertical: true),
                over: window
            )
        },
        ShotScene(id: "mac.welcome.add-code.adding", size: WelcomeWindow.size) {
            ShotStage(
                MacWelcomeWindow {
                    CodeRunningView(
                        openSteps: nil,
                        steps:
                            LatestStream.bootstrapSteps(
                                holding: .loading(
                                    progress: BridgeLoadProgress(
                                        step: .streamingLibrary,
                                        key:
                                            "core.library.load_step.streaming_library",
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
        libraries: [BridgeLibrary],
        savedCodes: [BridgeSavedRestoreEntry] = [],
        loadError: LoadError? = nil
    ) -> ShotScene {
        ShotScene(id: id, size: WelcomeWindow.size) {
            let reads = CountedReads(
                libraries: libraries,
                savedCodes: savedCodes
            )
            return ShotStage(
                PreviewScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup,
                    loadError: loadError
                )
            ) { reads.answered == 2 }
        }
    }

    /// The welcome window over `libraries`, with the dialog `laid` over it.
    /// `laid` is given the window's reads, to count the ones the dialog makes
    /// itself, and the window; `extraReads` is how many of those it makes.
    private static func overWelcome<Dialog: View>(
        _ id: String,
        size: CGSize = WelcomeWindow.size,
        libraries: [BridgeLibrary] = PreviewData.boardLibraries,
        savedCodes: [BridgeSavedRestoreEntry] = [],
        extraReads: Int = 0,
        laid dialog: @escaping @MainActor (CountedReads, AnyView) -> Dialog
    ) -> ShotScene {
        ShotScene(id: id, size: size) {
            let reads = CountedReads(
                libraries: libraries,
                savedCodes: savedCodes
            )
            // `PreviewScenes.welcome` at `size`: a board may draw the window
            // taller than the app's, to fit a popover under its rows.
            let window = MacWelcomeWindow(size: size) {
                WelcomeView(
                    onLibraryReady: { _, _ in },
                    onOpenLibrary: { _ in },
                    onFinishReplacement: { _, _ in },
                    onResetFromCloud: { _ in },
                    initialMode: .choose,
                    canDeleteActiveLibrary: true
                )
            }
            .environment(reads.setup)
            .environment(PreviewData.welcomeRemovals())
            return ShotStage(dialog(reads, AnyView(window))) {
                reads.answered == 2 + extraReads
            }
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

    /// The restore codes the saved-codes boards list: a Dropbox library's, and
    /// one saved for a different library, which can't be used.
    private static var savedCodes: [BridgeSavedRestoreEntry] {
        [
            .readable(
                code: PreviewSavedRestoreCode(
                    BridgeSavedRestoreCodeInfo(
                        name: "Garden",
                        libraryId: "saved-garden",
                        location: .dropbox(
                            location: .appFolder,
                            accountLabel: "ana@example.com",
                            folderLabel: nil
                        ),
                        oauthAccess: .dropbox(access: .appFolder),
                        hasS3Credentials: false
                    )
                )
            ),
            .unreadable(libraryId: "7f3c9a1e", reason: .otherLibrary),
        ]
    }

    /// The confirmation `library` asks before it is removed, counted as
    /// `loss`.
    private static func removalAlert(
        _ id: String,
        _ library: BridgeLibrary,
        losing loss: RemovalLoss = .counted(holdsUnsent: false)
    ) -> ShotScene {
        overWelcome(id) { _, window in
            OverWindow.alert(
                LibraryRemovalAlerts.confirmation(
                    for: library,
                    losing: loss
                ),
                over: window
            )
        }
    }

    /// The confirmation to delete the first saved restore code, as the
    /// saved-codes section's alert words it (`SavedCodeDeletion`).
    private static func savedCodeDeletionAlert() -> ShotScene {
        overWelcome("mac.welcome.saved-code.delete", savedCodes: savedCodes) {
            _,
            window in
            OverWindow.alert(
                SavedCodeDeletion.alert {},
                over: window
            )
        }
    }

    /// The details popover for Home music over the welcome window, once its
    /// status is in. The popover opens under Home music's row: the board
    /// puts its right edge 21 points past the list's, which is 420 wide and
    /// centred.
    private static func detailsScene(
        _ id: String,
        _ status: BridgeLibraryStanding
    ) -> ShotScene {
        overWelcome(
            id,
            size: CGSize(width: WelcomeWindow.size.width, height: 680),
            extraReads: 1
        ) { reads, window in
            OverWindow.popover(
                PreviewScenes.welcomeLibraryDetails(
                    PreviewData.homeMusic,
                    readStanding: reads.readStanding(status)
                ),
                top: 402,
                trailing: (WelcomeWindow.size.width - 420) / 2 - 21,
                over: window
            )
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
    /// The restore codes the saved-code listing answers.
    let savedCodes: [BridgeSavedRestoreEntry]

    init(
        libraries: [BridgeLibrary] = [],
        savedCodes: [BridgeSavedRestoreEntry] = []
    ) {
        self.libraries = libraries
        self.savedCodes = savedCodes
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
                return savedCodes
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

// Libraries only the boards show.
extension PreviewData {
    /// The libraries the welcome boards show: Home music, stored in the
    /// person's Dropbox, and Odile's records, which never synced.
    static let boardLibraries: [BridgeLibrary] = [homeMusic, odilesRecords]

    /// A library with no cloud home, which offers Delete in place of
    /// Remove.
    static let odilesRecords = BridgeLibrary(
        id: "lib-odiles-records",
        name: "Odile’s records",
        path: "/Users/preview/Music/bae/Odile’s records",
        state: .openable(storage: nil),
        isActive: false
    )
}

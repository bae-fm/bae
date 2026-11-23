import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The welcome window over this Mac's libraries or none, and the
    /// details, alerts, code sheets and steps that appear in it.
    static let welcomeScenes: [ShotScene] = [
        ShotScene(
            id: "mac.welcome.libraries",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(libraries: PreviewData.boardLibraries)
        },
        ShotScene(
            id: "mac.welcome.libraries.none",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(libraries: [])
        },
        ShotScene(
            id: "mac.welcome.libraries.damaged-join",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                unreadableInstallation: PreviewData.unreadableInstallationId
            )
        },
        ShotScene(
            id: "mac.welcome.open-failed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                loadError: LoadError(
                    .libraryOpen(offersReset: true),
                    PreviewData.displayErrorWithDetail,
                    libraryId: PreviewData.homeMusic.id
                )
            )
        },
        ShotScene(
            id: "mac.welcome.libraries.saved-codes",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeStage(
                libraries: PreviewData.boardLibraries,
                savedCodes: savedCodes
            )
        },
        ShotScene(
            id: "mac.welcome.library-details",
            size: detailsSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            detailsStage(PreviewData.standingSynced)
        },
        ShotScene(
            id: "mac.welcome.library-details.unsent-changes",
            size: detailsSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            detailsStage(PreviewData.standingUnsent)
        },
        ShotScene(
            id: "mac.welcome.delete-never-synced",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            removalAlertStage(PreviewData.odilesRecords)
        },
        ShotScene(
            id: "mac.welcome.remove",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            removalAlertStage(PreviewData.homeMusic)
        },
        ShotScene(
            id: "mac.welcome.remove.unsent-changes",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            removalAlertStage(
                PreviewData.homeMusic,
                losing: .counted(holdsUnsent: true)
            )
        },
        ShotScene(
            id: "mac.welcome.remove.removing",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
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
                ShotScenes.welcome(
                    initialMode: .choose,
                    setup: reads.setup,
                    removals: removals
                )
            ) { reads.answered == 2 }
        },
        ShotScene(
            id: "mac.welcome.remove.failed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            overWelcomeStage { _, window in
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
            }
        },
        ShotScene(
            id: "mac.welcome.saved-code.delete",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            overWelcomeStage(savedCodes: savedCodes) { _, window in
                OverWindow.alert(
                    SavedCodeDeletion.alert(
                        for: SavedRestoreEntry(savedCodes[0])
                    ) {},
                    over: window
                )
            }
        },
        ShotScene(
            id: "mac.welcome.opening.finding-libraries",
            size: loadingSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow(size: loadingSize) {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.findingLibraries.title,
                        leading: nil,
                        opening: nil,
                        reset: nil
                    )
                }
            )
        },
        ShotScene(
            id: "mac.welcome.opening.steps",
            size: loadingSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow(size: loadingSize) {
                    MacWelcomeOpening<BridgeOpenProgress>(
                        title: AppScreen.LoadingWork.opening.title,
                        leading: nil,
                        opening:
                            LatestStream.openProgress(
                                holding: BridgeOpenProgress(
                                    step: .rebuilding,
                                    key: "core.library.open_step.rebuilding",
                                    done: 45,
                                    total: 100,
                                    fraction: 0.45
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
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(MacWelcomeWindow { KeychainLockedView(onRetry: {}) })
        },
        ShotScene(
            id: "mac.welcome.reset.offered",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            // The reset's own line: a device that must be reset has no error
            // from opening the library, as when Settings offers it.
            ShotStage(ShotScenes.deviceReset(error: nil))
        },
        ShotScene(
            id: "mac.welcome.opening.resetting",
            size: loadingSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let progress = BridgeLoadProgress(
                step: .streamingLibrary,
                key: "core.library.load_step.streaming_library",
                done: 5,
                total: 10,
                fraction: 0.5
            )
            return ShotStage(
                MacWelcomeWindow(size: loadingSize) {
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
        ShotScene(
            id: "mac.welcome.reset.failed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                ShotScenes.deviceReset(
                    error: LoadError(
                        .deviceReset,
                        PreviewData.displayErrorRetryable
                    )
                )
            )
        },
        ShotScene(
            id: "mac.welcome.reset.unfinished",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
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
        ShotScene(
            id: "mac.welcome.create.name",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                ShotScenes.welcome(
                    initialMode: .create,
                    setup: PreviewData.welcomeSetup()
                )
            )
        },
        ShotScene(
            id: "mac.welcome.join.name",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
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
        ShotScene(
            id: "mac.welcome.join.waiting",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
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
        ShotScene(
            id: "mac.welcome.add-this-device.waiting",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow {
                    PairingWaitingView(
                        status: .waitingForApproval(confirmation: "412 778"),
                        steps: nil,
                        onCancel: {}
                    )
                }
            )
        },
        ShotScene(
            id: "mac.welcome.add-this-device.ended",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow {
                    PairingEndedView(onOK: {})
                }
            )
        },
        ShotScene(
            id: "mac.welcome.join.ended",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow {
                    JoinEndedView(inviter: "Ana", onOK: {})
                }
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.entering",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeSheetStage(
                EnterCodeSheet(form: enterCodeForm(), onCancel: {})
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.scanning",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            welcomeSheetStage(
                CodeScannerSheet(
                    onScan: { _ in },
                    onDismiss: {},
                    showing: .requesting
                )
            )
        },
        ShotScene(
            id: "mac.welcome.add-code.half-typed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let form = enterCodeForm()
            form.edit(code: String(PreviewData.restoreCode.prefix(25)))
            return welcomeSheetStage(EnterCodeSheet(form: form, onCancel: {}))
        },
        ShotScene(
            id: "mac.welcome.add-code.s3-key-needed",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            let form = enterCodeForm(reading: .restore(metadata: s3Metadata))
            form.acceptCode(PreviewData.restoreCode)
            form.accessKey = "AKIA4EXAMPLE"
            return welcomeSheetStage(RestoreS3KeySheet(form: form))
        },
        ShotScene(
            id: "mac.welcome.add-code.adding",
            size: WelcomeWindow.size,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
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
                                        total: 10,
                                        fraction: 0.3
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

    /// The window a library loads in, at the size the boards draw it: the
    /// app opens it at the library window's size fitted to the screen, which
    /// is this or more.
    private static let loadingSize = MainWindow.minSize

    /// The first screen over `libraries`, with `removals` as the work
    /// under way.
    static func welcomeStage(
        libraries: [BridgeLibrary],
        savedCodes: [BridgeSavedRestoreEntry] = [],
        loadError: LoadError? = nil,
        unreadableInstallation: String? = nil,
        removals: LibraryRemovals = PreviewData.welcomeRemovals()
    ) -> ShotStage {
        let reads = CountedReads(
            libraries: libraries,
            savedCodes: savedCodes,
            unreadableInstallation: unreadableInstallation
        )
        return ShotStage(
            ShotScenes.welcome(
                initialMode: .choose,
                setup: reads.setup,
                removals: removals,
                loadError: loadError
            )
        ) { reads.answered == reads.reads }
    }

    /// A native sheet over the first-run welcome window. The sheet draws
    /// its own scrim over the window, including the window buttons.
    private static func welcomeSheetStage(_ sheet: some View) -> ShotStage {
        let welcome = welcomeStage(libraries: [])
        return ShotStage(
            welcome.view.sheet(isPresented: .constant(true)) { sheet },
            isReady: welcome.isReady,
            isDrawn: { host in
                guard let window = host.window, window.attachedSheet != nil
                else { return false }
                return window.childWindows?.contains { $0 is ScrimWindow }
                    == true
            }
        )
    }

    /// The library list with the dialog `laid` over it.
    /// `extraReads` counts the reads the dialog makes itself.
    private static func overWelcomeStage<Dialog: View>(
        size: CGSize = WelcomeWindow.size,
        savedCodes: [BridgeSavedRestoreEntry] = [],
        extraReads: Int = 0,
        laid dialog: @escaping @MainActor (CountedReads, AnyView) -> Dialog
    ) -> ShotStage {
        let reads = CountedReads(
            libraries: PreviewData.boardLibraries,
            savedCodes: savedCodes
        )
        // `ShotScenes.welcome` at `size`: a board may draw the window
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
            reads.answered == reads.reads + extraReads
        }
    }

    /// The Enter code sheet's state: the code is read as `info` when the
    /// person chooses Continue, and nothing it starts reaches a library.
    static func enterCodeForm(
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
                    host: BaeHost.inTemporaryFolder(),
                    deviceName: "Preview Device"
                )
            },
            unfinishedJoins: { [] },
            unfinishedRestores: { [] },
            unfinishedJoinFor: { _ in nil },
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
    /// `loss`, over the welcome window.
    private static func removalAlertStage(
        _ library: BridgeLibrary,
        losing loss: RemovalLoss = .counted(holdsUnsent: false)
    ) -> ShotStage {
        overWelcomeStage { _, window in
            OverWindow.alert(
                LibraryRemovalAlerts.confirmation(
                    for: library,
                    losing: loss
                ),
                over: window
            )
        }
    }

    private static let detailsSize = CGSize(
        width: WelcomeWindow.size.width,
        height: 680
    )

    /// The details popover for Home music over the welcome window, once its
    /// status is in. The popover opens under Home music's row: the board
    /// puts its right edge 21 points past the list's, which is 420 wide and
    /// centred.
    private static func detailsStage(
        _ status: BridgeLibraryStanding
    ) -> ShotStage {
        overWelcomeStage(
            size: detailsSize,
            extraReads: 1
        ) { reads, window in
            OverWindow.popover(
                ShotScenes.welcomeLibraryDetails(
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
    /// The library whose unfinished join the listing of joins fails on, as
    /// unreadable.
    let unreadableInstallation: String?

    init(
        libraries: [BridgeLibrary] = [],
        savedCodes: [BridgeSavedRestoreEntry] = [],
        unreadableInstallation: String? = nil
    ) {
        self.libraries = libraries
        self.savedCodes = savedCodes
        self.unreadableInstallation = unreadableInstallation
    }

    /// How many reads the welcome window makes as it appears: the two lists,
    /// and the listing of joins when it fails.
    var reads: Int { unreadableInstallation == nil ? 2 : 3 }

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
            unfinishedJoins: { [self] in
                guard let unreadableInstallation else { return [] }
                await answer()
                throw BridgeError.Diagnostic(
                    category: .unfinishedInstallationUnreadable(
                        libraryId: unreadableInstallation
                    ),
                    detail: "invalid store settings: EOF while parsing"
                )
            },
            unfinishedJoinFor: { _ in throw UnservedReadError() },
            unfinishedRestores: { [] },
            discardUnfinishedInstallation: { _ in throw UnservedReadError() },
            makeBootstrap: {
                LibraryBootstrap(
                    host: BaeHost.inTemporaryFolder(),
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

    /// The library whose unfinished join the welcome boards show as one this
    /// Mac can't read.
    static let unreadableInstallationId = "3b9e07d4-51a2-4c8f-b6e3-0f2d9a7c4e81"

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

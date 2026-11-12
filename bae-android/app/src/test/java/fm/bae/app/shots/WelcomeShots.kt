package fm.bae.app.shots

import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.AppScreen
import fm.bae.app.DisplayError
import fm.bae.app.FailedStep
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.R
import fm.bae.app.ScreenFailure
import fm.bae.app.ui.InProgressScreen
import fm.bae.app.ui.ResetOfferedScreen
import fm.bae.app.ui.ResetUnfinishedScreen
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.removeLibraryParts
import fm.bae.app.ui.onboarding.CodeEntry
import fm.bae.app.ui.onboarding.JoinEndedStep
import fm.bae.app.ui.onboarding.JoinNameStep
import fm.bae.app.ui.onboarding.JoinStart
import fm.bae.app.ui.onboarding.LibraryDetailsRows
import fm.bae.app.ui.onboarding.OnboardingProgress
import fm.bae.app.ui.onboarding.OnboardingStep
import fm.bae.app.ui.onboarding.StandingRead
import fm.bae.app.ui.onboarding.WelcomeActions
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeSavedCodesScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.app.ui.onboarding.deleteSavedCodeParts
import fm.bae.app.ui.onboarding.removalFailedParts
import fm.bae.app.ui.testRecoveryLines
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Test
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeLibraryStanding
import uniffi.bae_bridge.BridgeLoadProgress
import uniffi.bae_bridge.BridgeLoadStep
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.BridgeStandingState
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import java.time.Instant

/** The shots of the ready android.welcome.* boards. */
@OptIn(ExperimentalMaterial3Api::class)
class WelcomeShots : BoardShot() {
    /** The welcome of a phone holding no library: the logo over the tagline, Scan code and Enter code at the foot. */
    @Test
    fun androidWelcomeLibrariesNone() = shot("android.welcome.libraries.none") { ScreenScene { WelcomeScene() } }

    /** The welcome on a phone that holds two libraries, one of them never synced. */
    @Test
    fun androidWelcomeLibraries() = shot("android.welcome.libraries") { ScreenScene { WelcomeLibrariesScene() } }

    /** The welcome with restore codes saved for libraries that aren't on the phone, each row ending in its ⋮ menu. */
    @Test
    fun androidWelcomeLibrariesSavedCodes() = shot("android.welcome.libraries.saved-codes") { ScreenScene { WelcomeSavedCodesScene() } }

    /** A library's details sheet, over a Dropbox library that synced two hours ago. */
    @Test
    fun androidWelcomeLibraryDetails() =
        shot("android.welcome.library-details") {
            LibraryDetailsScene(
                BridgeStandingState.Sync(syncShown(BridgeSyncStatusWord.SYNCED, "core.sync.synced", BridgeSyncTone.SUCCESS)),
            )
        }

    /** A library's details sheet, over a library holding changes it hasn't uploaded. */
    @Test
    fun androidWelcomeLibraryDetailsUnsentChanges() =
        shot("android.welcome.library-details.unsent-changes") { LibraryDetailsScene(BridgeStandingState.UnsentChanges) }

    /** The Add library sheet over the welcome's list: Scan code and Enter code. */
    @Test
    fun androidWelcomeAddLibrary() =
        shot("android.welcome.add-library") {
            SheetScene(stringResource(R.string.welcome_add_library), behind = { WelcomeLibrariesScene() }) {
                WelcomeActions(onScan = {}, onEnter = {})
            }
        }

    /** The confirmation to remove a library with nothing unsent. */
    @Test
    fun androidWelcomeRemove() =
        shot("android.welcome.remove") {
            InPlaceDialogScene(behind = { WelcomeLibrariesScene() }) {
                removeLibraryParts(sceneLibrary, RemovalLoss.Counted(holdsUnsent = false), {}, {})
            }
        }

    /** The confirmation to remove a library that holds changes it hasn't sent. */
    @Test
    fun androidWelcomeRemoveUnsentChanges() =
        shot("android.welcome.remove.unsent-changes") {
            InPlaceDialogScene(behind = { WelcomeLibrariesScene() }) {
                removeLibraryParts(sceneLibrary, RemovalLoss.Counted(holdsUnsent = true), {}, {})
            }
        }

    /** The confirmation to delete a saved restore code, which goes from this phone only. */
    @Test
    fun androidWelcomeSavedCodeDelete() =
        shot("android.welcome.saved-code.delete") {
            InPlaceDialogScene(behind = { WelcomeSavedCodesScene() }) {
                deleteSavedCodeParts({}, {})
            }
        }

    /** The welcome while a library is being removed: its row shows the step, with a spinner where the ⋮ was. */
    @Test
    fun androidWelcomeRemoveRemoving() = shot("android.welcome.remove.removing") { ScreenScene { WelcomeRemovingScene() } }

    /** The dialog for a removal that failed. */
    @Test
    fun androidWelcomeRemoveFailed() =
        shot("android.welcome.remove.failed") {
            InPlaceDialogScene(behind = { WelcomeLibrariesScene() }) {
                removalFailedParts(
                    sceneLibrary,
                    DisplayError("Could not reach the cloud. Check your connection and try again."),
                    onRetry = {},
                    onCancel = {},
                )
            }
        }

    /** Reading which libraries this device holds: the spinner under its words. */
    @Test
    fun androidWelcomeOpeningFindingLibraries() =
        shot("android.welcome.opening.finding-libraries") {
            ScreenScene { InProgressScreen(AppScreen.Loading(AppScreen.LoadingWork.FINDING_LIBRARIES)) }
        }

    /** Opening found this device's copy unusable: why, centred, with Reset from cloud…. */
    @Test
    fun androidWelcomeResetOffered() =
        shot("android.welcome.reset.offered") {
            ScreenScene {
                ResetOfferedScreen(
                    libraryId = "home",
                    error =
                        ScreenFailure(
                            FailedStep.LIBRARY_OPEN,
                            DisplayError("This device must be reset before it can sync."),
                        ),
                    lines = testRecoveryLines,
                    prepare = { error("a shot was tapped") },
                    errors = LocaleErrorDisplays(LocalContext.current),
                    onReset = {},
                    onLeave = {},
                )
            }
        }

    /** The step after an invite's code reads: the joining person's name, empty, above the keyboard's place. */
    @Test
    fun androidWelcomeJoinName() =
        shot("android.welcome.join.name") {
            ScreenScene {
                JoinNameStep(
                    step =
                        OnboardingStep.JoinName(
                            code = "invite-code",
                            invite =
                                BridgeInviteInfo(
                                    metadata =
                                        BridgeCodeMetadata(
                                            libraryId = "home",
                                            memberId = "ana",
                                            provider = BridgeCloudProvider.DROPBOX,
                                            location = sceneDropbox,
                                            oauthAccess = null,
                                            carriesS3Key = false,
                                        ),
                                    libraryName = "Home music",
                                    inviter = "Ana",
                                ),
                            entry = CodeEntry.NewDevice,
                        ),
                    error = null,
                    onJoin = {},
                    onBack = {},
                )
            }
        }

    /** A join that ended without letting this device in: the step that says so and whom to ask. */
    @Test
    fun androidWelcomeJoinEnded() =
        shot("android.welcome.join.ended") {
            ScreenScene {
                JoinEndedStep(
                    step =
                        OnboardingStep.JoinEnded(
                            invite =
                                BridgeInviteInfo(
                                    metadata =
                                        BridgeCodeMetadata(
                                            libraryId = "home",
                                            memberId = "ana",
                                            provider = BridgeCloudProvider.DROPBOX,
                                            location = sceneDropbox,
                                            oauthAccess = null,
                                            carriesS3Key = false,
                                        ),
                                    libraryName = "Home music",
                                    inviter = "Ana",
                                ),
                            entry = CodeEntry.NewDevice,
                        ),
                    onBack = {},
                )
            }
        }

    /** Opening a library that rebuilds its indexes: the step over its bar. */
    @Test
    fun androidWelcomeOpeningSteps() =
        shot("android.welcome.opening.steps") {
            val progress = BridgeOpenProgress(BridgeOpenStep.REBUILDING, "core.library.open_step.rebuilding", 4uL, 9uL)
            ScreenScene { InProgressScreen(AppScreen.Opening(LatestStream.openProgress(progress).only)) }
        }

    /** A device reset loading the library from storage again: the step it runs over its bar. */
    @Test
    fun androidWelcomeOpeningResetting() =
        shot("android.welcome.opening.resetting") {
            ScreenScene { InProgressScreen(AppScreen.Resetting(BridgeBootstrapStatus.Loading, loadingSteps())) }
        }

    /** A reset that stopped partway: the reset's line, centred, with Finish reset from cloud. */
    @Test
    fun androidWelcomeResetUnfinished() =
        shot("android.welcome.reset.unfinished") {
            ScreenScene {
                ResetUnfinishedScreen(error = null, lines = testRecoveryLines, onFinish = {}, onLeave = {})
            }
        }

    /** A pairing code scanned or typed on the welcome, adding this device: the step it runs, and Cancel. */
    @Test
    fun androidWelcomeAddCodeAdding() =
        shot("android.welcome.add-code.adding") {
            ScreenScene {
                OnboardingProgress(
                    step =
                        OnboardingStep.Running(
                            from =
                                OnboardingStep.Restore(
                                    code = "restore-code",
                                    metadata =
                                        BridgeCodeMetadata(
                                            libraryId = "home",
                                            memberId = "ana",
                                            provider = BridgeCloudProvider.DROPBOX,
                                            location = sceneDropbox,
                                            oauthAccess = null,
                                            carriesS3Key = false,
                                        ),
                                    entry = CodeEntry.NewDevice,
                                ),
                            state = BridgeBootstrapState(BridgeBootstrapStatus.Loading, cancellable = true),
                            steps = loadingSteps(),
                        ),
                    onCancel = {},
                    onBack = {},
                )
            }
        }

    /** A join waiting for approval: the step that says whom to tell the code, over the code and the line it waits on. */
    @Test
    fun androidWelcomeJoinWaiting() =
        shot("android.welcome.join.waiting") {
            ScreenScene {
                OnboardingProgress(
                    step =
                        OnboardingStep.Running(
                            from =
                                OnboardingStep.Invite(
                                    start = JoinStart.Code("invite-code", "Dan"),
                                    invite =
                                        BridgeInviteInfo(
                                            metadata =
                                                BridgeCodeMetadata(
                                                    libraryId = "home",
                                                    memberId = "ana",
                                                    provider = BridgeCloudProvider.DROPBOX,
                                                    location = sceneDropbox,
                                                    oauthAccess = null,
                                                    carriesS3Key = false,
                                                ),
                                            libraryName = "Home music",
                                            inviter = "Ana",
                                        ),
                                    entry = CodeEntry.NewDevice,
                                ),
                            state =
                                BridgeBootstrapState(
                                    status =
                                        BridgeBootstrapStatus.WaitingForApproval(
                                            confirmation = "412 778",
                                        ),
                                    cancellable = false,
                                ),
                            steps = LatestStream.bootstrapSteps().only,
                        ),
                    onCancel = null,
                    onBack = {},
                )
            }
        }
}

private fun loadingSteps() =
    LatestStream
        .bootstrapSteps(
            BridgeBootstrapStep.Loading(
                BridgeLoadProgress(
                    BridgeLoadStep.STREAMING_LIBRARY,
                    "core.library.load_step.streaming_library",
                    3uL,
                    10uL,
                ),
            ),
        ).only

@Composable
private fun LibraryDetailsScene(state: BridgeStandingState) {
    SheetScene("Home music", behind = { WelcomeLibrariesScene() }) {
        LibraryDetailsRows(
            storage = sceneDropbox,
            standing = StandingRead.Loaded(BridgeLibraryStanding(state, Instant.now().minusSeconds(TWO_HOURS_SECONDS))),
        )
    }
}

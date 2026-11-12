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
import fm.bae.app.ui.testRecoveryLines
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.RemoveLibraryText
import fm.bae.app.ui.onboarding.CodeEntry
import fm.bae.app.ui.onboarding.JoinNameStep
import fm.bae.app.ui.onboarding.JoinStart
import fm.bae.app.ui.onboarding.LibraryDetailsRows
import fm.bae.app.ui.onboarding.OnboardingProgress
import fm.bae.app.ui.onboarding.OnboardingStep
import fm.bae.app.ui.onboarding.RemovalFailedText
import fm.bae.app.ui.onboarding.StandingRead
import fm.bae.app.ui.onboarding.WelcomeActions
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeSavedCodesScene
import fm.bae.app.ui.onboarding.WelcomeScene
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
    fun androidWelcomeLibrariesNone() = shot("android.welcome.libraries.none") { WelcomeScene() }

    /** The welcome on a phone that holds two libraries, one of them never synced. */
    @Test
    fun androidWelcomeLibraries() = shot("android.welcome.libraries") { WelcomeLibrariesScene() }

    /** The welcome with restore codes saved for libraries that aren't on the phone, each row ending in its ⋮ menu. */
    @Test
    fun androidWelcomeLibrariesSavedCodes() = shot("android.welcome.libraries.saved-codes") { WelcomeSavedCodesScene() }

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
            SheetScene(stringResource(R.string.welcome_add_library)) { WelcomeActions(onScan = {}, onEnter = {}) }
        }

    /** The confirmation to remove a library with nothing unsent. */
    @Test
    fun androidWelcomeRemove() =
        shot("android.welcome.remove") {
            DialogScene { RemoveLibraryText(sceneLibrary, RemovalLoss.Counted(holdsUnsent = false)) }
        }

    /** The confirmation to remove a library that holds changes it hasn't sent. */
    @Test
    fun androidWelcomeRemoveUnsentChanges() =
        shot("android.welcome.remove.unsent-changes") {
            DialogScene { RemoveLibraryText(sceneLibrary, RemovalLoss.Counted(holdsUnsent = true)) }
        }

    /** The welcome while a library is being removed: its row shows the step, with a spinner where the ⋮ was. */
    @Test
    fun androidWelcomeRemoveRemoving() = shot("android.welcome.remove.removing") { WelcomeRemovingScene() }

    /** The dialog for a removal that failed. */
    @Test
    fun androidWelcomeRemoveFailed() =
        shot("android.welcome.remove.failed") {
            DialogScene {
                RemovalFailedText(
                    sceneLibrary,
                    DisplayError("Couldn’t reach the cloud. Check your connection and try again."),
                )
            }
        }

    /** Reading which libraries this device holds: the spinner under its words. */
    @Test
    fun androidWelcomeOpeningFindingLibraries() =
        shot("android.welcome.opening.finding-libraries") {
            ScreenScene { InProgressScreen(AppScreen.Loading(AppScreen.LoadingWork.FINDING_LIBRARIES)) }
        }

    /** Opening found this device's copy unusable: why, centred, with Reset this device…. */
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
                                    expiresAt = Instant.ofEpochSecond(JOIN_NOW_SECONDS + ONE_DAY_SECONDS),
                                ),
                            entry = CodeEntry.NewDevice,
                        ),
                    error = null,
                    onJoin = {},
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

    /** A reset that stopped partway: the reset's line, centred, with Finish resetting. */
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
                )
            }
        }

    /** A join waiting for approval: its confirmation number and until when it waits. */
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
                                            expiresAt = Instant.ofEpochSecond(JOIN_NOW_SECONDS + ONE_DAY_SECONDS),
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
                )
            }
        }
}

private fun loadingSteps() =
    LatestStream
        .bootstrapSteps(
            BridgeBootstrapStep.Loading(
                BridgeLoadProgress(BridgeLoadStep.READING, "core.library.load_step.reading", 2uL, 8uL),
            ),
        ).only

private const val JOIN_NOW_SECONDS = 1_800_000_000L
private const val ONE_DAY_SECONDS = 86_400L

@Composable
private fun LibraryDetailsScene(state: BridgeStandingState) {
    SheetScene("Home music") {
        LibraryDetailsRows(
            storage = sceneDropbox,
            standing = StandingRead.Loaded(BridgeLibraryStanding(state, Instant.now().minusSeconds(TWO_HOURS_SECONDS))),
        )
    }
}

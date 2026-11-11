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
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.RemoveLibraryText
import fm.bae.app.ui.onboarding.CodeEntry
import fm.bae.app.ui.onboarding.JoinStart
import fm.bae.app.ui.onboarding.LibraryDetailsRows
import fm.bae.app.ui.onboarding.OnboardingProgress
import fm.bae.app.ui.onboarding.OnboardingStep
import fm.bae.app.ui.onboarding.RemovalFailedText
import fm.bae.app.ui.onboarding.StandingRead
import fm.bae.app.ui.onboarding.WelcomeActions
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import fm.bae.app.ui.onboarding.WelcomeRemovingScene
import fm.bae.app.ui.onboarding.WelcomeScene
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Test
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeLibraryStanding
import uniffi.bae_bridge.BridgeNextCheck
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
                    prepare = { error("a shot was tapped") },
                    errors = LocaleErrorDisplays(LocalContext.current),
                    onReset = {},
                    onLeave = {},
                )
            }
        }

    /** A join waiting for approval: its confirmation number, until when it waits, when it checks next. */
    @Test
    fun androidWelcomeJoinWaiting() =
        shot("android.welcome.join.waiting") {
            ScreenScene {
                OnboardingProgress(
                    step =
                        OnboardingStep.Running(
                            from =
                                OnboardingStep.Invite(
                                    start = JoinStart.Code("invite-code"),
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
                                            invitee = "Ana",
                                            expiresAt = Instant.ofEpochSecond(JOIN_NOW_SECONDS + ONE_DAY_SECONDS),
                                        ),
                                    entry = CodeEntry.NewDevice,
                                ),
                            state =
                                BridgeBootstrapState(
                                    status =
                                        BridgeBootstrapStatus.WaitingForApproval(
                                            confirmation = "412 778",
                                            nextCheck =
                                                BridgeNextCheck.At(
                                                    Instant.ofEpochSecond(JOIN_NOW_SECONDS + JOIN_NEXT_CHECK_SECONDS),
                                                ),
                                        ),
                                    cancellable = false,
                                ),
                            steps = LatestStream.bootstrapSteps().only,
                        ),
                    onCancel = null,
                    onCheckNow = {},
                    nowMs = { JOIN_NOW_SECONDS * MILLIS_PER_SECOND },
                )
            }
        }
}

private const val JOIN_NOW_SECONDS = 1_800_000_000L
private const val JOIN_NEXT_CHECK_SECONDS = 42L
private const val ONE_DAY_SECONDS = 86_400L
private const val MILLIS_PER_SECOND = 1_000L

@Composable
private fun LibraryDetailsScene(state: BridgeStandingState) {
    SheetScene("Home music") {
        LibraryDetailsRows(
            storage = sceneDropbox,
            standing = StandingRead.Loaded(BridgeLibraryStanding(state, Instant.now().minusSeconds(TWO_HOURS_SECONDS))),
        )
    }
}

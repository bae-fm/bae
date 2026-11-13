package fm.bae.app.ui.onboarding

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.LibraryOpenProgress
import fm.bae.app.data.LibrarySetupStatus
import fm.bae.app.deviceName
import fm.bae.fastvalues.LatestSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHostInterface
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeOAuthAccess
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.BridgeUnfinishedRestore
import uniffi.bae_bridge.decodeCodeInfo

private const val TAG = "bae.OnboardingFlow"
private val logger = BaeLogger(TAG)

/**
 * What the person enters a code for: a new device, or a restore over this
 * device's copy of a library ([RestoreOver]). On a new device the code decides:
 * the person's own restore code or pairing code adds this device to their
 * library, and an invite joins someone's. A restore over this device's copy
 * takes no invite and no pairing code.
 */
internal sealed interface CodeEntry {
    /** A new device, on the welcome screen. */
    data object NewDevice : CodeEntry
}

/**
 * Where onboarding stands: a code to scan, type or pick from those saved on
 * this phone, a code read and waiting on the person, or a restore or join
 * running.
 */
internal sealed interface OnboardingStep {
    /** No code yet: the person scans or types one for [entry]. */
    data class EnterCode(
        val entry: CodeEntry,
    ) : OnboardingStep

    /**
     * What a restore or join for [entry] runs from: a code the person scanned
     * or typed, one saved on this phone, or what core kept for a restore that
     * stopped partway.
     */
    sealed interface Attempt {
        val entry: CodeEntry
    }

    /**
     * A code the person scanned or typed for [entry]: a restore, whose form
     * waits on the person when the code was typed or lacks its S3 key, or an
     * invite, which asks the person's name before it joins.
     */
    sealed interface EnteredCode : Attempt

    /**
     * A code of the person's own, read, that adds this device to their library:
     * their restore code or a pairing code. S3 asks for its access key before
     * going on unless the code holds it, as a QR code from the person's own
     * device does.
     */
    sealed interface OwnCode :
        EnteredCode,
        OnboardingStep {
        val code: String
        val metadata: BridgeCodeMetadata

        /** Whether going on needs the S3 access key typed in: an S3 code that doesn't hold it. */
        val needsS3Key: Boolean
            get() = metadata.provider == BridgeCloudProvider.S3 && !metadata.carriesS3Key
    }

    /** The person's own restore code, read. */
    data class Restore(
        override val code: String,
        override val metadata: BridgeCodeMetadata,
        override val entry: CodeEntry,
    ) : OwnCode

    /**
     * A pairing code another of the person's devices shows, read. This device
     * asks it to add this one and waits for its approval.
     */
    data class Pair(
        override val code: String,
        override val metadata: BridgeCodeMetadata,
        override val entry: CodeEntry,
    ) : OwnCode

    /**
     * A pairing that ended without letting this device in, whether it was
     * declined, expired or closed on the other device: one step that says so
     * and what to do. OK, and Back, return to the code screen.
     */
    data class PairingEnded(
        val entry: CodeEntry,
    ) : OnboardingStep

    /**
     * A restore code coven saved on this phone, chosen in the welcome's list.
     * It restores at once unless its S3 library has no key saved beside it,
     * which the person types in first.
     */
    data class RestoreSaved(
        val saved: SavedRestoreCode,
    ) : Attempt,
        OnboardingStep {
        /** Saved codes are listed on a new device's welcome only. */
        override val entry: CodeEntry
            get() = CodeEntry.NewDevice
    }

    /**
     * A restore over this device's copy that stopped partway, finished from
     * what core kept: no code, no sign-in.
     */
    data class FinishRestore(
        override val entry: RestoreOver,
    ) : Attempt

    /**
     * A restore onto this device that stopped after it recorded itself,
     * finished from what core kept: no code, and a sign-in only when the kept
     * one is gone or refused.
     */
    data class ResumeRestore(
        val restore: BridgeUnfinishedRestore,
    ) : Attempt {
        /** Only a new device's welcome goes on with one. */
        override val entry: CodeEntry
            get() = CodeEntry.NewDevice
    }

    /**
     * An invite's code, read, whose person still has to give their name
     * before joining: the name the join's request carries. Going back
     * returns to the code screen.
     */
    data class JoinName(
        val code: String,
        val invite: BridgeInviteInfo,
        override val entry: CodeEntry,
    ) : EnteredCode,
        OnboardingStep

    /**
     * A join that ended without letting this device in, whether the invite
     * was declined, cancelled or expired, or the device that made it is gone:
     * one step that says so and whom to ask for a new one. OK, and Back,
     * return to the code screen.
     */
    data class JoinEnded(
        val invite: BridgeInviteInfo,
        val entry: CodeEntry,
    ) : OnboardingStep

    /**
     * An invite joining, from a name just given, or a join under way that
     * continues with the name its request carries: joining starts at once,
     * with no form, from [start].
     */
    data class Invite(
        val start: JoinStart,
        val invite: BridgeInviteInfo,
        override val entry: CodeEntry,
    ) : EnteredCode

    /**
     * A restore with [from], or a join when [from] is an invite, running;
     * [state] is core's latest report of where it stands and whether it can
     * still be cancelled, null until the first arrives, and [steps] the
     * counted step it runs, opening or loading, which its line draws each
     * frame. A restore over this device's copy just opens it when this device
     * holds the code's keys, and [openSteps] are that open's steps; null for
     * any other attempt.
     */
    data class Running(
        val from: Attempt,
        val state: BridgeBootstrapState?,
        val steps: LatestSource<BridgeBootstrapStep>,
        val openSteps: LatestSource<BridgeOpenProgress>? = null,
    ) : OnboardingStep
}

/**
 * How a code reached onboarding: a scanned code starts as soon as it holds all
 * it needs, since the person scanned it to add this device; a typed restore
 * code shows what it read before the person adds.
 */
internal enum class CodeSource { SCANNED, TYPED }

/**
 * Where a join starts: an invite's code with the name the person gave, or a
 * join this device has under way, which asks for no name.
 */
internal sealed interface JoinStart {
    data class Code(
        val code: String,
        val name: String,
    ) : JoinStart

    data class Unfinished(
        val join: BridgeUnfinishedJoin,
    ) : JoinStart
}

/**
 * What a code read for [entry] asks of the person: the person's own restore
 * code restores, and their pairing code asks the device that shows it to add
 * this one; an invite asks their name, then joins the library it names. A
 * restore over this device's copy takes no invite and no pairing code: the
 * code goes to the restore, and core says so.
 */
internal fun codeReadStep(
    code: String,
    info: BridgeCodeInfo,
    entry: CodeEntry,
): OnboardingStep.EnteredCode =
    when (info) {
        is BridgeCodeInfo.Restore -> {
            OnboardingStep.Restore(code, info.metadata, entry)
        }

        is BridgeCodeInfo.Pairing -> {
            when (entry) {
                is RestoreOver -> {
                    OnboardingStep.Restore(code, info.metadata, entry)
                }

                CodeEntry.NewDevice -> {
                    OnboardingStep.Pair(code, info.metadata, entry)
                }
            }
        }

        is BridgeCodeInfo.Invite -> {
            when (entry) {
                is RestoreOver -> {
                    OnboardingStep.Restore(code, info.invite.metadata, entry)
                }

                CodeEntry.NewDevice -> {
                    OnboardingStep.JoinName(code, info.invite, entry)
                }
            }
        }
    }

/**
 * The core onboarding runs against: the [host] that restores and joins,
 * [errorDisplay], which words a failed call as core says to show it,
 * [newCancellation], which makes the stop each restore is handed,
 * [newOpenProgress], which makes the open a restore over this device's copy
 * reports its steps in, and [newSetupStatus], which makes the setup each
 * restore or join says where it stands through. [joins] is what a join asks of
 * core beside the [host].
 */
internal class OnboardingCore(
    val host: BridgeHostInterface,
    val errorDisplay: (BridgeException) -> DisplayError?,
    val newCancellation: () -> BootstrapCancellation,
    val newOpenProgress: () -> LibraryOpenProgress,
    val newSetupStatus: () -> LibrarySetupStatus,
    val joins: JoinCore = JoinCore(),
)

/**
 * What a join asks of core beside the host: [libraryId] names the library a join under way is for, and
 * [newControl] makes the control each join runs with.
 */
internal class JoinCore(
    val libraryId: (BridgeUnfinishedJoin) -> String = { it.info().metadata.libraryId },
    val newControl: () -> BridgeJoinControl = ::BridgeJoinControl,
)

/**
 * Onboarding's one flow: a code, scanned or typed, is either the person's own
 * restore code or an invite, and this device restores or joins with it; a
 * restore code saved on this phone restores from the welcome's list. With
 * [restoreOver], the code restores over this device's copy of that library. What either produced goes to
 * [onBootstrapped]; an invite that ended without letting this device in returns
 * to the code screen with that said.
 */
internal class OnboardingFlow(
    private val scope: CoroutineScope,
    private val context: Context,
    private val core: OnboardingCore,
    private val oauthProvidersError: String?,
    restoreOver: RestoreOver?,
    private val onBootstrapped: (BridgeBootstrapResult) -> Unit,
) {
    var step by mutableStateOf<OnboardingStep>(OnboardingStep.EnterCode(restoreOver ?: CodeEntry.NewDevice))
        private set
    var error by mutableStateOf<DisplayError?>(null)

    /** The unfinished join or restore this phone can't read, which listing what is under way failed on. */
    val unreadable =
        UnreadableInstallation(
            scope,
            core.host,
            onFailed = { error = core.errorDisplay(it) },
            listAgain = {
                error = null
                resumeUnfinished()
            },
        )

    /**
     * The restore codes saved on this phone, which the welcome lists; null for
     * a restore over this device's copy, which takes the person's current code.
     */
    val savedCodes: SavedRestoreCodes? =
        if (restoreOver == null) SavedRestoreCodes(scope, core.host, core.errorDisplay) else null

    private var cancellation: (() -> Unit)? = null

    /** The running join's control; null when no join runs. */
    private var joining: BridgeJoinControl? = null

    /**
     * Whether the running join's waiting screen shows. A join control made
     * while it does starts held, and core holds the join's frequent checks
     * for approval exactly as long as this says so.
     */
    private var waitingScreenShown = false

    init {
        when (restoreOver?.start) {
            // A join this device had under way when bae stopped opens on its
            // waiting screen and goes on checking; with none, a restore that
            // stopped partway goes on finishing.
            null -> {
                resumeUnfinished()
            }

            // Finishing needs nothing from the person, and only it opens the
            // library.
            RestoreOver.Start.FINISH -> {
                finishRestore(restoreOver)
            }

            RestoreOver.Start.CODE -> {}
        }
    }

    /** Finish the restore over this device's copy that stopped partway, from what core kept. */
    private fun finishRestore(entry: RestoreOver) {
        val running = core.newCancellation()
        run(OnboardingStep.FinishRestore(entry), running::cancel) { setup ->
            core.host.resumeReplacement(entry.libraryId, BridgeReplacedBy.RESTORE, setup, running)
        }
    }

    /**
     * Go on with what this device has under way, one at a time: the join, if
     * any, or else the first restore that stopped after it recorded itself.
     */
    private fun resumeUnfinished() {
        scope.launch {
            try {
                val join = core.host.unfinishedJoins().firstOrNull()
                if (join != null) {
                    join(OnboardingStep.Invite(JoinStart.Unfinished(join), join.info(), CodeEntry.NewDevice))
                    return@launch
                }
                val restore = core.host.unfinishedRestores().firstOrNull() ?: return@launch
                resumeRestore(restore)
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("listing what is under way failed", e)
                val category = (e as? BridgeException.Diagnostic)?.category
                if (category is BridgeErrorCategory.UnfinishedInstallationUnreadable) {
                    unreadable.found(category.libraryId)
                } else {
                    error = core.errorDisplay(e)
                }
            }
        }
    }

    /** Finish [restore] from what core kept. */
    private fun resumeRestore(restore: BridgeUnfinishedRestore) {
        val running = core.newCancellation()
        run(OnboardingStep.ResumeRestore(restore), running::cancel) { setup ->
            core.host.resumeRestore(restore, setup, running)
        }
    }

    /**
     * Read [text], scanned or typed on [from] as [source] says, and move to
     * what its code asks of the person. A code from the person's own device
     * holds their S3 key.
     */
    fun enter(
        from: OnboardingStep.EnterCode,
        text: String,
        source: CodeSource,
    ) {
        val code = text.trim()
        val info =
            try {
                decodeCodeInfo(code)
            } catch (e: BridgeException) {
                logger.warning("the entered code is neither a restore code nor an invite", e)
                error = DisplayError(context.getString(R.string.onboarding_code_invalid))
                return
            }
        advance(from, code, info, source)
    }

    /** Move from [from] to what [code], which decodes to [info], asks of the person. */
    internal fun advance(
        from: OnboardingStep.EnterCode,
        code: String,
        info: BridgeCodeInfo,
        source: CodeSource,
    ) {
        val metadata =
            when (info) {
                is BridgeCodeInfo.Restore -> info.metadata
                is BridgeCodeInfo.Pairing -> info.metadata
                is BridgeCodeInfo.Invite -> info.invite.metadata
            }
        if (signInUnavailable(metadata.oauthAccess)) return
        error = null
        when (val read = codeReadStep(code, info, from.entry)) {
            // An invite goes on with a join under way for its library, which
            // asks no name (coven refuses a second, different one), or else
            // asks the person's name.
            is OnboardingStep.JoinName -> {
                goOnOrAskName(read)
            }

            is OnboardingStep.Invite -> {
                join(read)
            }

            is OnboardingStep.OwnCode -> {
                if (source == CodeSource.SCANNED && !read.needsS3Key) add(read, credentials = null) else step = read
            }
        }
    }

    /** Continue the join under way for [read]'s library, or else ask the person's name. */
    private fun goOnOrAskName(read: OnboardingStep.JoinName) {
        scope.launch {
            try {
                val library = read.invite.metadata.libraryId
                val underWay = core.host.unfinishedJoins().firstOrNull { core.joins.libraryId(it) == library }
                if (underWay == null) {
                    step = read
                } else {
                    join(OnboardingStep.Invite(JoinStart.Unfinished(underWay), read.invite, read.entry))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: BridgeException) {
                logger.error("listing the joins under way failed", e)
                error = core.errorDisplay(e)
            }
        }
    }

    /** Join as the person who typed [name] on [from]'s step, which the join's request carries. */
    fun join(
        from: OnboardingStep.JoinName,
        name: String,
    ) {
        join(OnboardingStep.Invite(JoinStart.Code(from.code, name.trim()), from.invite, from.entry))
    }

    /**
     * Restore from [saved], chosen in the welcome's list: at once when it holds
     * all it needs, as a scanned code does; an S3 library with no key saved
     * beside the code asks for the key first.
     */
    fun openSaved(saved: SavedRestoreCode) {
        if (signInUnavailable(saved.info.oauthAccess)) return
        error = null
        val read = OnboardingStep.RestoreSaved(saved)
        if (saved.needsS3Key) step = read else restore(read, credentials = null)
    }

    /**
     * Whether a restore or join needing [oauthAccess] can't start: a provider
     * that needs a sign-in can't be reached when the app's registrations
     * failed to load, which [error] then says, rather than starting.
     */
    private fun signInUnavailable(oauthAccess: BridgeOAuthAccess?): Boolean {
        if (oauthAccess == null || oauthProvidersError == null) return false
        error = DisplayError(context.coreString("ui.load_error.sign_in_settings.advice"), oauthProvidersError)
        return true
    }

    /** Back to the code screen from a code waiting on the person. */
    fun back(from: OnboardingStep.Attempt) {
        error = null
        step = OnboardingStep.EnterCode(from.entry)
    }

    /** Back to the code screen from an ended join. */
    fun back(from: OnboardingStep.JoinEnded) {
        error = null
        step = OnboardingStep.EnterCode(from.entry)
    }

    /** Back to the code screen from an ended pairing. */
    fun back(from: OnboardingStep.PairingEnded) {
        error = null
        step = OnboardingStep.EnterCode(from.entry)
    }

    /**
     * Add this device with [from], the person's restore code, or their pairing
     * code, which asks the device that shows it to add this one. Leaving the
     * screen of a pairing drops the call, which ends the pairing and removes
     * this device's copy.
     */
    fun add(
        from: OnboardingStep.OwnCode,
        credentials: BridgeS3Credentials?,
    ) {
        when (from) {
            is OnboardingStep.Restore -> {
                restore(from, credentials)
            }

            is OnboardingStep.Pair -> {
                val control = core.joins.newControl()
                // A new control checks on coven's doubling waits until told the screen shows.
                if (waitingScreenShown) control.setScreenShown(true)
                joining = control
                run(from, control::cancel) { setup ->
                    core.host.joinWithPairing(from.code, credentials, deviceName(context), setup, control)
                }
            }
        }
    }

    fun restore(
        from: OnboardingStep.Restore,
        credentials: BridgeS3Credentials?,
    ) {
        val running = core.newCancellation()
        when (val entry = from.entry) {
            is RestoreOver -> {
                val opening = core.newOpenProgress()
                run(from, running::cancel, opening) { setup ->
                    core.host.restoreReplacingDevice(
                        entry.libraryId,
                        from.code,
                        credentials,
                        deviceName(context),
                        setup,
                        opening.opening,
                        running,
                    )
                }
            }

            CodeEntry.NewDevice -> {
                run(from, running::cancel) { setup ->
                    core.host.restoreFromCode(from.code, credentials, deviceName(context), setup, running)
                }
            }
        }
    }

    /** Restore from a code saved on this phone, with the S3 key typed in when it needs one. */
    fun restore(
        from: OnboardingStep.RestoreSaved,
        credentials: BridgeS3Credentials?,
    ) {
        val running = core.newCancellation()
        run(from, running::cancel) { setup ->
            core.host.restoreFromSavedCode(from.saved.code, credentials, deviceName(context), setup, running)
        }
    }

    /**
     * Join from an invite's code, or resume a join under way. Leaving the
     * screen drops the call and keeps the join, which the next launch resumes.
     */
    private fun join(from: OnboardingStep.Invite) {
        val control = core.joins.newControl()
        // A new control checks on coven's doubling waits until told the screen shows.
        if (waitingScreenShown) control.setScreenShown(true)
        joining = control
        run(from, control::cancel) { setup ->
            when (val start = from.start) {
                is JoinStart.Code -> {
                    core.host.joinWithInvite(start.code, start.name, deviceName(context), setup, control)
                }

                is JoinStart.Unfinished -> {
                    core.host.resumeJoin(start.join, setup, control)
                }
            }
        }
    }

    /**
     * Whether the running join's waiting screen shows. While it does, the
     * join checks for approval every few seconds.
     */
    fun setWaitingScreenShown(shown: Boolean) {
        waitingScreenShown = shown
        joining?.setScreenShown(shown)
    }

    /**
     * Stop the running restore, or a join whose request isn't sent yet. coven
     * removes an installation it had not yet published, and the call ends as
     * cancelled, which returns to the code it started from.
     */
    fun cancel() {
        cancellation?.invoke()
    }

    /** Run [start], with [opening] following the open it runs, if it opens this device's own copy. */
    private fun run(
        from: OnboardingStep.Attempt,
        cancel: () -> Unit,
        opening: LibraryOpenProgress? = null,
        start: suspend (BridgeLibrarySetup) -> BridgeBootstrapResult,
    ) {
        error = null
        val setup = core.newSetupStatus()
        step = OnboardingStep.Running(from, state = null, steps = setup.steps, openSteps = opening?.steps)
        cancellation = cancel
        // Started even when the scope is already cancelled, so the setup made
        // above is closed by the `finally` below.
        scope.launch(start = CoroutineStart.ATOMIC) {
            val following =
                launch {
                    setup.followStates { state ->
                        val current = step
                        if (current is OnboardingStep.Running) step = current.copy(state = state)
                    }
                }
            try {
                val result = start(setup.setup)
                following.cancel()
                when (result) {
                    BridgeBootstrapResult.Declined -> {
                        step =
                            when (from) {
                                is OnboardingStep.Invite -> OnboardingStep.JoinEnded(from.invite, from.entry)
                                is OnboardingStep.Pair -> OnboardingStep.PairingEnded(from.entry)
                                else -> error("only a join or a pairing ends without letting it in")
                            }
                    }

                    // The cancel came as the request was sent: the join goes on.
                    BridgeBootstrapResult.JoinRequestSent -> {
                        resumeUnfinished()
                    }

                    // Another join or restore of this library is under way on this device; it goes on instead.
                    BridgeBootstrapResult.CallUnderWay -> {
                        resumeUnfinished()
                    }

                    // Deleted since the welcome listed it; the welcome reads
                    // the list again, without it.
                    BridgeBootstrapResult.SavedCodeRemoved -> {
                        step = OnboardingStep.EnterCode(from.entry)
                        error = DisplayError(context.getString(R.string.onboarding_saved_code_removed))
                    }

                    else -> {
                        onBootstrapped(result)
                    }
                }
            } catch (e: BridgeException.Cancelled) {
                logger.debug("onboarding cancelled by the person", e)
                following.cancel()
                step = from.formStep()
            } catch (e: BridgeException) {
                logger.error("restoring or joining failed", e)
                following.cancel()
                step = from.formStep()
                error = core.errorDisplay(e)
            } finally {
                following.cancel()
                setup.close()
                opening?.close()
                if (cancellation === cancel) {
                    cancellation = null
                    joining = null
                }
            }
        }
    }
}

/**
 * Where an ended restore or join returns: a restore's form, or the code
 * screen for an invite or the finishing of a restore that stopped partway,
 * which have none.
 */
private fun OnboardingStep.Attempt.formStep(): OnboardingStep =
    when (this) {
        is OnboardingStep.Restore -> {
            this
        }

        is OnboardingStep.Pair -> {
            this
        }

        is OnboardingStep.RestoreSaved -> {
            this
        }

        // A join refused after the person gave their name, such as a name core
        // refuses, returns to the name step, with its error.
        is OnboardingStep.Invite -> {
            when (val start = start) {
                is JoinStart.Code -> OnboardingStep.JoinName(start.code, invite, entry)
                is JoinStart.Unfinished -> OnboardingStep.EnterCode(entry)
            }
        }

        is OnboardingStep.JoinName -> {
            OnboardingStep.EnterCode(entry)
        }

        is OnboardingStep.FinishRestore -> {
            OnboardingStep.EnterCode(entry)
        }

        is OnboardingStep.ResumeRestore -> {
            OnboardingStep.EnterCode(entry)
        }
    }

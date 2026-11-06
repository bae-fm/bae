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
import fm.bae.app.displayError
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
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHostInterface
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeOAuthAccess
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.decodeCodeInfo

private const val TAG = "bae.OnboardingFlow"
private val logger = BaeLogger(TAG)

/**
 * What the person enters a code for: a new device, or a restore over this
 * device's copy of a library ([RestoreOver]). On a new device the code decides:
 * the person's own restore code adds this device to their library, and an
 * invite joins someone's. A restore over this device's copy takes no invite.
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
     * A code read for [entry]: one the person scanned or typed, or one saved
     * on this phone.
     */
    sealed interface CodeRead {
        val entry: CodeEntry
    }

    /**
     * A code the person scanned or typed for [entry]: a restore, whose form
     * waits on the person when the code was typed or lacks its S3 key, or an
     * invite, which joins at once.
     */
    sealed interface EnteredCode : CodeRead

    /**
     * The person's own restore code, read; S3 asks for its access key before
     * restoring unless the code holds it, as one from the person's own device
     * does.
     */
    data class Restore(
        val code: String,
        val metadata: BridgeCodeMetadata,
        override val entry: CodeEntry,
    ) : EnteredCode,
        OnboardingStep {
        /** Whether restoring needs the S3 access key typed in: an S3 code that doesn't hold it. */
        val needsS3Key: Boolean
            get() = metadata.provider == BridgeCloudProvider.S3 && !metadata.carriesS3Key
    }

    /**
     * A restore code coven saved on this phone, chosen in the welcome's list.
     * It restores at once unless its S3 library has no key saved beside it,
     * which the person types in first.
     */
    data class RestoreSaved(
        val saved: SavedRestoreCode,
    ) : CodeRead,
        OnboardingStep {
        /** Saved codes are listed on a new device's welcome only. */
        override val entry: CodeEntry
            get() = CodeEntry.NewDevice
    }

    /**
     * An invite, read or saved from a join under way: joining starts at once,
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
        val from: CodeRead,
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

/** Where a join starts: an invite's code, or a join this device has under way. */
internal sealed interface JoinStart {
    data class Code(
        val code: String,
    ) : JoinStart

    data class Unfinished(
        val join: BridgeUnfinishedJoin,
    ) : JoinStart
}

/**
 * What a code read for [entry] asks of the person: the person's own restore
 * code restores; an invite joins the library it names. A restore over this
 * device's copy takes no invite: the code goes to the restore, and core says
 * so.
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

        is BridgeCodeInfo.Invite -> {
            when (entry) {
                is RestoreOver -> {
                    OnboardingStep.Restore(code, info.invite.metadata, entry)
                }

                CodeEntry.NewDevice -> {
                    OnboardingStep.Invite(JoinStart.Code(code), info.invite, entry)
                }
            }
        }
    }

/**
 * The core onboarding runs against: the [host] that restores and joins,
 * [newCancellation], which makes the stop each restore is handed,
 * [newOpenProgress], which makes the open a restore over this device's copy
 * reports its steps in, and [newSetupStatus], which makes the setup each
 * restore or join says where it stands through.
 */
internal class OnboardingCore(
    val host: BridgeHostInterface,
    val newCancellation: () -> BootstrapCancellation,
    val newOpenProgress: () -> LibraryOpenProgress,
    val newSetupStatus: () -> LibrarySetupStatus,
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

    /**
     * The restore codes saved on this phone, which the welcome lists; null for
     * a restore over this device's copy, which takes the person's current code.
     */
    val savedCodes: SavedRestoreCodes? =
        if (restoreOver == null) SavedRestoreCodes(scope, core.host) { context.displayError(it) } else null

    private var cancellation: (() -> Unit)? = null

    /** The running join's control, for Check now; null when no join runs. */
    private var joining: BridgeJoinControl? = null

    init {
        // A join this device had under way when bae stopped opens on its
        // waiting screen and goes on checking.
        if (restoreOver == null) resumeUnfinished()
    }

    /** Resume the join this device has under way, if any. */
    private fun resumeUnfinished() {
        scope.launch {
            val joins =
                try {
                    core.host.unfinishedJoins()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: BridgeException) {
                    logger.error("listing the joins under way failed", e)
                    error = context.displayError(e)
                    return@launch
                }
            val join = joins.firstOrNull() ?: return@launch
            join(OnboardingStep.Invite(JoinStart.Unfinished(join), join.info(), CodeEntry.NewDevice))
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
                is BridgeCodeInfo.Invite -> info.invite.metadata
            }
        if (signInUnavailable(metadata.oauthAccess)) return
        error = null
        when (val read = codeReadStep(code, info, from.entry)) {
            // An invite joins at once: it carries the person's name.
            is OnboardingStep.Invite -> {
                join(read)
            }

            is OnboardingStep.Restore -> {
                if (source == CodeSource.SCANNED && !read.needsS3Key) restore(read, credentials = null) else step = read
            }
        }
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
    fun back(from: OnboardingStep.CodeRead) {
        error = null
        step = OnboardingStep.EnterCode(from.entry)
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
        val control = BridgeJoinControl()
        joining = control
        run(from, control::cancel) { setup ->
            when (val start = from.start) {
                is JoinStart.Code -> core.host.joinWithInvite(start.code, deviceName(context), setup, control)
                is JoinStart.Unfinished -> core.host.resumeJoin(start.join, setup, control)
            }
        }
    }

    /** End the running join's wait so it checks for approval now. */
    fun checkNow() {
        joining?.checkNow()
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
        from: OnboardingStep.CodeRead,
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
                        step = OnboardingStep.EnterCode(from.entry)
                        error = DisplayError(context.getString(R.string.onboarding_invite_ended))
                    }

                    // The cancel came as the request was sent: the join goes on.
                    BridgeBootstrapResult.JoinRequestSent -> {
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
                error = context.displayError(e)
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

/** Where an ended restore or join returns: a restore's form, or the code screen for an invite, which has none. */
private fun OnboardingStep.CodeRead.formStep(): OnboardingStep =
    when (this) {
        is OnboardingStep.Restore -> this
        is OnboardingStep.RestoreSaved -> this
        is OnboardingStep.Invite -> OnboardingStep.EnterCode(entry)
    }

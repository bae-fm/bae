package fm.bae.app.ui.onboarding

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ConflatedProgressDelivery
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.deviceName
import fm.bae.app.displayError
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BootstrapCallback
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHostInterface
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeJoinControl
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
 * Where onboarding stands: a code to scan or type, a code read and waiting on
 * the person, or a restore or join running.
 */
internal sealed interface OnboardingStep {
    /** No code yet: the person scans or types one for [entry]. */
    data class EnterCode(
        val entry: CodeEntry,
    ) : OnboardingStep

    /** A code read for [entry]: a restore, which waits on its form, or an invite, which joins at once. */
    sealed interface CodeRead {
        val entry: CodeEntry
    }

    /**
     * The person's own restore code, read; S3 asks for its access key before
     * restoring unless the code holds it, as one from the person's own device
     * does.
     */
    data class Restore(
        val code: String,
        val metadata: BridgeCodeMetadata,
        override val entry: CodeEntry,
    ) : CodeRead,
        OnboardingStep

    /**
     * An invite, read or saved from a join under way: joining starts at once,
     * with no form, from [start].
     */
    data class Invite(
        val start: JoinStart,
        val invite: BridgeInviteInfo,
        override val entry: CodeEntry,
    ) : CodeRead

    /**
     * A restore with [from], or a join when [from] is an invite, running;
     * [status] is coven's latest report, null until the first arrives.
     */
    data class Running(
        val from: CodeRead,
        val status: BridgeBootstrapStatus?,
    ) : OnboardingStep
}

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
): OnboardingStep.CodeRead =
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
 * Onboarding's one flow: a code, scanned or typed, is either the person's own
 * restore code or an invite, and this device restores or joins with it. With
 * [restoreOver], the code restores over this device's copy of that library. What either produced goes to
 * [onBootstrapped]; an invite declined or expired returns to the code screen
 * with that said.
 */
internal class OnboardingFlow(
    private val scope: CoroutineScope,
    private val context: Context,
    private val host: BridgeHostInterface,
    private val oauthProvidersError: String?,
    restoreOver: RestoreOver?,
    private val onBootstrapped: (BridgeBootstrapResult) -> Unit,
) {
    var step by mutableStateOf<OnboardingStep>(OnboardingStep.EnterCode(restoreOver ?: CodeEntry.NewDevice))
        private set
    var error by mutableStateOf<DisplayError?>(null)

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
                    host.unfinishedJoins()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: BridgeException) {
                    logger.error("listing the joins under way failed", e)
                    error = context.displayError(e) ?: DisplayError(e.toString())
                    return@launch
                }
            val join = joins.firstOrNull() ?: return@launch
            join(OnboardingStep.Invite(JoinStart.Unfinished(join), join.info(), CodeEntry.NewDevice))
        }
    }

    /**
     * Read [text], scanned or typed on [from], and move to what its code asks
     * of the person. A code from the person's own device holds their S3 key.
     */
    fun enter(
        from: OnboardingStep.EnterCode,
        text: String,
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
        val metadata =
            when (info) {
                is BridgeCodeInfo.Restore -> info.metadata
                is BridgeCodeInfo.Invite -> info.invite.metadata
            }
        // A provider that needs a sign-in can't be reached when the app's
        // registrations failed to load; say why rather than starting.
        if (metadata.oauthAccess != null && oauthProvidersError != null) {
            error = DisplayError(context.coreString("ui.load_error.sign_in_settings.advice"), oauthProvidersError)
            return
        }
        error = null
        when (val read = codeReadStep(code, info, from.entry)) {
            // An invite joins at once: it carries the person's name.
            is OnboardingStep.Invite -> join(read)

            is OnboardingStep.Restore -> step = read
        }
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
        val running = BootstrapCancellation()
        run(from, running::cancel) { callback ->
            when (val entry = from.entry) {
                is RestoreOver -> {
                    host.restoreReplacingDevice(
                        entry.libraryId,
                        from.code,
                        credentials,
                        deviceName(context),
                        callback,
                        running,
                    )
                }

                CodeEntry.NewDevice -> {
                    host.restoreFromCode(from.code, credentials, deviceName(context), callback, running)
                }
            }
        }
    }

    /**
     * Join from an invite's code, or resume a join under way. Leaving the
     * screen drops the call and keeps the join, which the next launch resumes.
     */
    private fun join(from: OnboardingStep.Invite) {
        val control = BridgeJoinControl()
        joining = control
        run(from, control::cancel) { callback ->
            when (val start = from.start) {
                is JoinStart.Code -> host.joinWithInvite(start.code, deviceName(context), callback, control)
                is JoinStart.Unfinished -> host.resumeJoin(start.join, callback, control)
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

    private fun run(
        from: OnboardingStep.CodeRead,
        cancel: () -> Unit,
        start: suspend (BootstrapCallback) -> BridgeBootstrapResult,
    ) {
        error = null
        step = OnboardingStep.Running(from, status = null)
        cancellation = cancel
        scope.launch {
            val statuses =
                ConflatedProgressDelivery<BridgeBootstrapStatus>(scope) { status ->
                    val current = step
                    if (current is OnboardingStep.Running) step = current.copy(status = status)
                }
            try {
                val result =
                    start(
                        object : BootstrapCallback {
                            override fun onStatus(status: BridgeBootstrapStatus) {
                                statuses.offer(status)
                            }
                        },
                    )
                statuses.closeAndJoin()
                when (result) {
                    BridgeBootstrapResult.Declined -> {
                        step = OnboardingStep.EnterCode(from.entry)
                        error = DisplayError(context.getString(R.string.onboarding_invite_declined))
                    }

                    // The cancel came as the request was sent: the join goes on.
                    BridgeBootstrapResult.JoinRequestSent -> {
                        resumeUnfinished()
                    }

                    else -> {
                        onBootstrapped(result)
                    }
                }
            } catch (e: BridgeException.Cancelled) {
                logger.debug("onboarding cancelled by the person", e)
                statuses.closeAndJoin()
                step = from.formStep()
            } catch (e: CancellationException) {
                statuses.close()
                throw e
            } catch (e: BridgeException) {
                logger.error("restoring or joining failed", e)
                statuses.closeAndJoin()
                step = from.formStep()
                error = context.displayError(e) ?: DisplayError(e.toString())
            } finally {
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
        is OnboardingStep.Invite -> OnboardingStep.EnterCode(entry)
    }

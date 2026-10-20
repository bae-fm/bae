package fm.bae.app.ui.onboarding

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ConflatedProgressDelivery
import fm.bae.app.R
import fm.bae.app.deviceName
import fm.bae.app.localizedLine
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
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.decodeCodeInfo
import uniffi.bae_bridge.readScannedCode

private const val TAG = "bae.OnboardingFlow"
private val logger = BaeLogger(TAG)

/**
 * What the person enters a code for: one of a new device's two choices, or a
 * restore over this device's copy of a library ([RestoreOver]). Whichever it
 * is, the code itself decides between restoring and joining, except that a
 * restore over this device's copy takes no invite.
 */
internal sealed interface CodeEntry {
    /** Join someone's library with the invite an admin sent. */
    data object JoinLibrary : CodeEntry

    /**
     * Add this device to the person's own library with the code Add device
     * shows on another of their devices, or their recovery code.
     */
    data object AddDevice : CodeEntry
}

/**
 * Where onboarding stands: a new device's two choices, a code to scan or type,
 * a code read and waiting on the person, or a restore or join running.
 */
internal sealed interface OnboardingStep {
    /** A new device's first screen: join someone's library, or add this device to the person's own. */
    data object Choose : OnboardingStep

    /** No code yet: the person scans or types one for [entry]. */
    data class EnterCode(
        val entry: CodeEntry,
    ) : OnboardingStep

    /** A code read for [entry] and waiting on the person. */
    sealed interface CodeRead : OnboardingStep {
        val entry: CodeEntry
    }

    /**
     * The person's own restore code, read; S3 asks for its access key before
     * restoring unless [s3Credentials] came beside the code from the person's
     * own device.
     */
    data class Restore(
        val code: String,
        val metadata: BridgeCodeMetadata,
        val s3Credentials: BridgeS3Credentials?,
        override val entry: CodeEntry,
    ) : CodeRead

    /** An invite to [libraryName], read; joining asks for the person's name. */
    data class Invite(
        val code: String,
        val metadata: BridgeCodeMetadata,
        val libraryName: String,
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

/**
 * What a code read for [entry] asks of the person: the person's own restore
 * code restores, with any S3 key that came beside it; an invite joins the
 * library it names. Which choice the person made doesn't matter, but a restore
 * over this device's copy takes no invite: the code goes to the restore, and
 * core says so.
 */
internal fun codeReadStep(
    code: String,
    info: BridgeCodeInfo,
    s3Credentials: BridgeS3Credentials?,
    entry: CodeEntry,
): OnboardingStep.CodeRead =
    when (info) {
        is BridgeCodeInfo.Restore -> {
            OnboardingStep.Restore(code, info.metadata, s3Credentials, entry)
        }

        is BridgeCodeInfo.Invite -> {
            when (entry) {
                is RestoreOver -> {
                    OnboardingStep.Restore(code, info.metadata, s3Credentials, entry)
                }

                CodeEntry.JoinLibrary, CodeEntry.AddDevice -> {
                    OnboardingStep.Invite(code, info.metadata, info.libraryName, entry)
                }
            }
        }
    }

/**
 * Onboarding's one flow: a new device's person chooses to join a library or
 * add this device to their own; then a code, scanned or typed, is either the
 * person's own restore code or an invite, and this device restores or joins
 * with it. With [restoreOver], the flow starts at the code, for a restore over
 * this device's copy of that library. What either produced goes to
 * [onBootstrapped]; an invite declined or expired returns to the code screen
 * with that said.
 */
internal class OnboardingFlow(
    private val scope: CoroutineScope,
    private val context: Context,
    private val host: BridgeHost,
    private val oauthProvidersError: String?,
    restoreOver: RestoreOver?,
    private val onBootstrapped: (BridgeBootstrapResult) -> Unit,
) {
    var step by mutableStateOf<OnboardingStep>(restoreOver?.let(OnboardingStep::EnterCode) ?: OnboardingStep.Choose)
        private set
    var error by mutableStateOf<String?>(null)

    private var cancellation: BootstrapCancellation? = null

    /** Move from the first screen to scanning or typing a code for [entry]. */
    fun choose(entry: CodeEntry) {
        error = null
        step = OnboardingStep.EnterCode(entry)
    }

    /** Back to a new device's first screen from its code screen. */
    fun backToChoices() {
        error = null
        step = OnboardingStep.Choose
    }

    /**
     * Read [text], scanned or typed on [from], and move to what its code asks
     * of the person. A scan from the person's own device can carry their S3
     * key beside the code.
     */
    fun enter(
        from: OnboardingStep.EnterCode,
        text: String,
    ) {
        val (scanned, info) =
            try {
                val scanned = readScannedCode(text)
                scanned to decodeCodeInfo(scanned.code)
            } catch (e: BridgeException) {
                logger.warning("the entered code is neither a restore code nor an invite", e)
                error = context.getString(R.string.onboarding_code_invalid)
                return
            }
        val metadata =
            when (info) {
                is BridgeCodeInfo.Restore -> info.metadata
                is BridgeCodeInfo.Invite -> info.metadata
            }
        // A provider that needs a sign-in can't be reached when the app's
        // registrations failed to load; say why rather than starting.
        if (metadata.oauthAccess != null && oauthProvidersError != null) {
            error = oauthProvidersError
            return
        }
        error = null
        step = codeReadStep(scanned.code, info, scanned.s3Credentials, from.entry)
    }

    /** Back to the code screen from a code waiting on the person. */
    fun back(from: OnboardingStep.CodeRead) {
        error = null
        step = OnboardingStep.EnterCode(from.entry)
    }

    fun restore(
        from: OnboardingStep.Restore,
        credentials: BridgeS3Credentials?,
    ) = run(from) { callback, cancellation ->
        when (val entry = from.entry) {
            is RestoreOver -> {
                host.restoreReplacingDevice(
                    entry.libraryId,
                    from.code,
                    credentials,
                    deviceName(context),
                    callback,
                    cancellation,
                )
            }

            CodeEntry.JoinLibrary, CodeEntry.AddDevice -> {
                host.restoreFromCode(from.code, credentials, deviceName(context), callback, cancellation)
            }
        }
    }

    fun join(
        from: OnboardingStep.Invite,
        personName: String,
    ) = run(from) { callback, cancellation ->
        host.joinWithInvite(from.code, deviceName(context), personName, callback, cancellation)
    }

    /**
     * Stop the running restore or join. coven removes an installation it had
     * not yet published, and the call ends as cancelled, which returns to the
     * code it started from.
     */
    fun cancel() {
        cancellation?.cancel()
    }

    private fun run(
        from: OnboardingStep.CodeRead,
        start: suspend (BootstrapCallback, BootstrapCancellation) -> BridgeBootstrapResult,
    ) {
        error = null
        step = OnboardingStep.Running(from, status = null)
        val running = BootstrapCancellation()
        cancellation = running
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
                        running,
                    )
                statuses.closeAndJoin()
                when (result) {
                    BridgeBootstrapResult.Declined -> {
                        step = OnboardingStep.EnterCode(from.entry)
                        error = context.getString(R.string.onboarding_invite_declined)
                    }

                    else -> {
                        onBootstrapped(result)
                    }
                }
            } catch (e: BridgeException.Cancelled) {
                logger.debug("onboarding cancelled by the person", e)
                statuses.closeAndJoin()
                step = from
            } catch (e: CancellationException) {
                statuses.close()
                throw e
            } catch (e: BridgeException) {
                logger.error("restoring or joining failed", e)
                statuses.closeAndJoin()
                step = from
                error = context.localizedLine(e) ?: e.toString()
            } finally {
                if (cancellation === running) cancellation = null
            }
        }
    }
}

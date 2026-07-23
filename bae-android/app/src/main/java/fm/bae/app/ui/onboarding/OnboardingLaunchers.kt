package fm.bae.app.ui.onboarding

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInviteCodeInfo
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.JoinFromCodeOperation
import uniffi.bae_bridge.RestoreFromCodeOperation
import uniffi.bae_bridge.availableCloudProviders
import uniffi.bae_bridge.decodeInviteCode
import uniffi.bae_bridge.decodeRestoreCode
import uniffi.bae_bridge.generateJoinRequest
import uniffi.bae_bridge.joinFromCodeOperation
import uniffi.bae_bridge.restoreFromCodeOperation

private const val TAG = "bae.OnboardingLaunchers"
private val logger = BaeLogger(TAG)

private class LinkFlow(
    val job: Job,
) {
    var restoreOperation: RestoreFromCodeOperation? = null

    suspend fun execute(
        code: String,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
        context: Context,
        onLinked: (BridgeLibrary) -> Unit,
    ) {
        val info = decodeRestoreCode(code)
        val oauthTokenJson =
            if (info.needsOauth) {
                resolveOauthToken(oauthLinking, oauthLinkingError, context, info.cloudProvider)
            } else {
                null
            }
        val operation = restoreFromCodeOperation(code = code, oauthTokenJson = oauthTokenJson)
        restoreOperation = operation
        val libraryInfo = withContext(Dispatchers.IO) { operation.restore() }
        onLinked(libraryInfo)
    }

    fun cancel() {
        restoreOperation?.cancel()
        job.cancel()
    }
}

/**
 * One in-progress join attempt: join from the invite code on a background
 * thread, using the OAuth token the joiner already obtained when it picked the
 * provider. Mirrors [LinkFlow], except it joins as a new member rather than
 * restoring this device's own library. The blocking `join()` bridge call can't
 * be interrupted by coroutine cancellation, so [cancel] also cancels the
 * operation's own token.
 */
private class JoinFlow(
    val job: Job,
) {
    var joinOperation: JoinFromCodeOperation? = null

    suspend fun execute(
        code: String,
        joinRequestCode: String,
        oauthTokenJson: String?,
        onJoined: (BridgeLibrary) -> Unit,
    ) {
        // OAuth (and the account-email fetch) already ran when the provider was
        // picked; the token captured then is reused here — no second sign-in.
        val operation =
            joinFromCodeOperation(code = code, joinRequestCode = joinRequestCode, oauthTokenJson = oauthTokenJson)
        joinOperation = operation
        val libraryInfo = withContext(Dispatchers.IO) { operation.join() }
        onJoined(libraryInfo)
    }

    fun cancel() {
        joinOperation?.cancel()
        job.cancel()
    }
}

private suspend fun resolveOauthToken(
    oauthLinking: OAuthLinker?,
    oauthLinkingError: String?,
    context: Context,
    provider: BridgeCloudProvider,
): String {
    val oauthError =
        oauthLinkingError
            ?: if (oauthLinking == null) context.getString(R.string.onboarding_oauth_unconfigured) else null
    if (oauthError != null) throw IllegalStateException(oauthError)
    return oauthLinking!!.authorize(context, provider)
}

/**
 * Holds the in-progress link attempt and its UI state (error text, whether a
 * link is running). Created once per onboarding via [remember] so its snapshot
 * state survives recomposition; the screen reads [error]/[isLinking] and drives
 * [link]/[cancel]. A new attempt cancels any prior one; the finally block only
 * clears state if it still owns the current attempt (identity check), so a
 * superseded attempt's completion can't wipe a newer one's state.
 */
internal class LinkLauncher(
    private val scope: CoroutineScope,
    private val context: Context,
    private val onLinked: (BridgeLibrary) -> Unit,
) {
    var error by mutableStateOf<String?>(null)
    private var flow by mutableStateOf<LinkFlow?>(null)
    val isLinking: Boolean get() = flow != null

    fun link(
        code: String,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ) {
        error = null
        flow?.cancel()
        lateinit var started: LinkFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    started.execute(code, oauthLinking, oauthLinkingError, context, onLinked)
                } catch (e: BridgeException.Cancelled) {
                    logger.debug("link flow cancelled by bridge", e)
                } catch (e: CancellationException) {
                    logger.debug("link flow coroutine cancelled", e)
                    throw e
                } catch (e: Exception) {
                    error = e.toString()
                } finally {
                    if (flow === started) flow = null
                }
            }
        started = LinkFlow(launched)
        flow = started
        launched.start()
    }

    fun cancel() {
        flow?.cancel()
        flow = null
    }
}

/**
 * The join twin of [LinkLauncher], plus the up-front provider step joining needs.
 * The joiner first picks the target library's provider ([selectProvider]); for an
 * OAuth provider that authenticates and fetches the account email, then generates
 * this device's join-request with the email baked in. The captured token is held
 * and reused by [join] — no second sign-in. Same attempt/supersede/cleanup
 * discipline as [LinkLauncher] for the join itself.
 */
class JoinLauncher(
    private val scope: CoroutineScope,
    private val context: Context,
    private val onJoined: (BridgeLibrary) -> Unit,
) {
    var error by mutableStateOf<String?>(null)
    private var flow by mutableStateOf<JoinFlow?>(null)
    val isJoining: Boolean get() = flow != null

    // Provider-prep state, read by JoinLibraryScreen.
    var provider by mutableStateOf<BridgeCloudProvider?>(null)
        private set
    var requestCode by mutableStateOf<String?>(null)
        private set
    var fingerprint by mutableStateOf<String?>(null)
        private set
    var generateError by mutableStateOf<String?>(null)
        private set
    var isAuthorizing by mutableStateOf(false)
        private set

    // The invite code the approving device handed back, and its live decode:
    // null before anything is entered, success once it parses, failure when it
    // doesn't. Both drive the preview rows and the join gate.
    var inviteInput by mutableStateOf("")
        private set
    private var decodedInvite by mutableStateOf<Result<BridgeInviteCodeInfo>?>(null)

    // Whether this edition can connect to any OAuth provider. The S3-only
    // edition's bindings offer only S3, so an OAuth library can't be joined here.
    private val oauthSupported: Boolean = availableCloudProviders().any(::providerUsesOauth)

    // The token/email captured at provider selection: the token is reused for
    // the join, the email is baked into the generated code.
    private var oauthTokenJson: String? = null
    private var prepJob: Job? = null

    /** The preview and join-gate state derived from the current invite input. */
    val invitePreview: JoinInvitePreview
        get() = joinInvitePreview(decodedInvite, oauthSupported, oauthTokenJson != null)

    /** Whether the join action should be enabled for the current input. */
    val joinReady: Boolean get() = joinEnabled(invitePreview)

    /**
     * Record the invite code the joiner typed or scanned and decode it live for
     * the preview. Decoding is a local parse, so it runs inline as the code
     * changes; a failure surfaces as the invalid-code preview, not a throw.
     */
    fun updateInvite(raw: String) {
        inviteInput = raw
        error = null
        val trimmed = raw.trim()
        decodedInvite = if (trimmed.isEmpty()) null else runCatching { decodeInviteCode(trimmed) }
    }

    /**
     * Pick the provider the target library uses and prepare this device's
     * join-request. For an OAuth provider this authenticates and fetches the
     * account email up front; for S3/iCloud it generates the code with no email.
     */
    fun selectProvider(
        provider: BridgeCloudProvider,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ) {
        prepJob?.cancel()
        error = null
        generateError = null
        oauthTokenJson = null
        requestCode = null
        fingerprint = null
        this.provider = provider
        prepJob =
            scope.launch {
                try {
                    var email: String? = null
                    if (providerUsesOauth(provider)) {
                        isAuthorizing = true
                        val oauthError =
                            oauthLinkingError
                                ?: if (oauthLinking == null) {
                                    context.getString(R.string.onboarding_oauth_unconfigured)
                                } else {
                                    null
                                }
                        if (oauthError != null) {
                            generateError = oauthError
                            isAuthorizing = false
                            return@launch
                        }
                        val token = oauthLinking!!.authorize(context, provider)
                        email = oauthLinking.fetchAccountEmail(provider, token)
                        oauthTokenJson = token
                        isAuthorizing = false
                    }
                    val request = withContext(Dispatchers.IO) { generateJoinRequest(email) }
                    requestCode = request.code
                    fingerprint = request.fingerprint
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logger.error("Failed to prepare join request", e)
                    isAuthorizing = false
                    generateError = e.message ?: context.getString(R.string.onboarding_join_generate_failed)
                }
            }
    }

    /** Return to the provider picker, dropping the generated code and token. */
    fun resetProvider() {
        prepJob?.cancel()
        provider = null
        requestCode = null
        fingerprint = null
        generateError = null
        isAuthorizing = false
        oauthTokenJson = null
        inviteInput = ""
        decodedInvite = null
        error = null
    }

    fun join() {
        val code = inviteInput.trim()
        // The code generated for this device's own join-request, minted back
        // when generateJoinRequest ran in selectProvider — coven needs it back
        // to promote that pending identity into this store's custody.
        val joinRequestCode = requestCode ?: return
        // Reuse the token captured at provider selection — no second OAuth.
        val oauthTokenJson = this.oauthTokenJson
        error = null
        flow?.cancel()
        lateinit var started: JoinFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    started.execute(code, joinRequestCode, oauthTokenJson, onJoined)
                } catch (e: BridgeException.Cancelled) {
                    logger.debug("join flow cancelled by bridge", e)
                } catch (e: CancellationException) {
                    logger.debug("join flow coroutine cancelled", e)
                    throw e
                } catch (e: Exception) {
                    error = e.toString()
                } finally {
                    if (flow === started) flow = null
                }
            }
        started = JoinFlow(launched)
        flow = started
        launched.start()
    }

    fun cancel() {
        flow?.cancel()
        flow = null
    }
}

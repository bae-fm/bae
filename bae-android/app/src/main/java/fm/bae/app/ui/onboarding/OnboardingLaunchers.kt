package fm.bae.app.ui.onboarding

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.ConflatedProgressDelivery
import fm.bae.app.OAuthLinker
import fm.bae.app.R
import fm.bae.app.localizedLine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeDevicePairingOffer
import uniffi.bae_bridge.BridgeDevicePairingPhase
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeJoiningDeviceJoinProgress
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.JoinDevicePairingOperation
import uniffi.bae_bridge.JoiningDeviceJoinProgressCallback
import uniffi.bae_bridge.RestoreFromCodeOperation
import uniffi.bae_bridge.abandonPendingDevicePairingJoin
import uniffi.bae_bridge.decodeDevicePairingOffer
import uniffi.bae_bridge.decodeRestoreCode
import uniffi.bae_bridge.joinDevicePairingOperation
import uniffi.bae_bridge.pendingDevicePairingJoin
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
        onLinked(withContext(Dispatchers.IO) { operation.restore() })
    }

    fun cancel() {
        restoreOperation?.cancel()
        job.cancel()
    }
}

private class JoinFlow(
    val job: Job,
) {
    var operation: JoinDevicePairingOperation? = null

    suspend fun execute(
        pairingCode: String,
        oauthTokenJson: String?,
        onPrepared: (String) -> Unit,
        onProgress: (BridgeJoiningDeviceJoinProgress) -> Unit,
        onJoined: (BridgeLibrary) -> Unit,
    ) {
        val started =
            joinDevicePairingOperation(
                pairingCode = pairingCode,
                oauthTokenJson = oauthTokenJson,
            )
        operation = started
        onPrepared(started.fingerprint())
        onJoined(
            withContext(Dispatchers.IO) {
                started.join(
                    object : JoiningDeviceJoinProgressCallback {
                        override fun onProgress(progress: BridgeJoiningDeviceJoinProgress) {
                            onProgress(progress)
                        }
                    },
                )
            },
        )
    }

    fun cancel() {
        operation?.cancel()
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

/** The joining device's one-code pairing flow. */
class JoinLauncher(
    private val scope: CoroutineScope,
    private val context: Context,
    private val onJoined: (BridgeLibrary) -> Unit,
) {
    var error by mutableStateOf<String?>(null)
    private var flow by mutableStateOf<JoinFlow?>(null)
    val isJoining: Boolean get() = flow != null

    var pairingCode by mutableStateOf("")
        private set
    var decodedOffer by mutableStateOf<Result<BridgeDevicePairingOffer>?>(null)
        private set
    var isAuthorizing by mutableStateOf(false)
        private set
    var joiningFingerprint by mutableStateOf<String?>(null)
        private set
    var joinProgress by mutableStateOf<BridgeJoiningDeviceJoinProgress?>(null)
        private set

    private var oauthTokenJson: String? = null
    private var authorizationJob: Job? = null
    var hasPendingPairing by mutableStateOf(false)
        private set

    val joinReady: Boolean
        get() {
            if (isJoining) return false
            val offer = decodedOffer?.getOrNull() ?: return false
            return !offer.needsOauth || oauthTokenJson != null
        }

    fun updatePairingCode(
        raw: String,
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ) {
        if (hasPendingPairing) return
        authorizationJob?.cancel()
        pairingCode = raw
        error = null
        joiningFingerprint = null
        joinProgress = null
        hasPendingPairing = false
        oauthTokenJson = null
        isAuthorizing = false
        val trimmed = raw.trim()
        if (trimmed.isEmpty()) {
            decodedOffer = null
        } else {
            val decoded = runCatching { decodeDevicePairingOffer(trimmed) }
            decodedOffer = decoded
            val offer = decoded.getOrNull()
            if (offer?.needsOauth == true) {
                isAuthorizing = true
                authorizationJob =
                    scope.launch {
                        try {
                            oauthTokenJson =
                                resolveOauthToken(
                                    oauthLinking,
                                    oauthLinkingError,
                                    context,
                                    offer.cloudProvider,
                                )
                            isAuthorizing = false
                        } catch (e: CancellationException) {
                            throw e
                        } catch (e: Exception) {
                            logger.error("Failed to authorize device pairing", e)
                            isAuthorizing = false
                            error = e.message ?: context.getString(R.string.onboarding_oauth_unconfigured)
                        }
                    }
            }
        }
    }

    fun reset() {
        authorizationJob?.cancel()
        flow?.cancel()
        flow = null
        isAuthorizing = false
        oauthTokenJson = null
        pairingCode = ""
        decodedOffer = null
        error = null
        joiningFingerprint = null
        joinProgress = null
        hasPendingPairing = false
    }

    fun join() {
        decodedOffer?.getOrNull() ?: return
        val code = pairingCode.trim()
        val token = oauthTokenJson
        error = null
        joiningFingerprint = null
        flow?.cancel()
        lateinit var started: JoinFlow
        val launched =
            scope.launch(start = CoroutineStart.LAZY) {
                val progressDelivery =
                    ConflatedProgressDelivery<BridgeJoiningDeviceJoinProgress>(scope) {
                        joinProgress = it
                    }
                try {
                    started.execute(
                        code,
                        token,
                        onPrepared = {
                            hasPendingPairing = true
                            joiningFingerprint = it
                            joinProgress = BridgeJoiningDeviceJoinProgress.WaitingForApproval
                        },
                        onProgress = { progress ->
                            progressDelivery.offer(progress)
                        },
                        onJoined,
                    )
                } catch (e: BridgeException.Cancelled) {
                    // Core reports Cancelled only for this device's own cancel;
                    // a join the other end walked away from arrives as a named
                    // reason below. Nothing to tell the user about their own tap.
                    logger.debug("pairing join cancelled locally", e)
                } catch (e: CancellationException) {
                    logger.debug("pairing join coroutine cancelled", e)
                    throw e
                } catch (e: BridgeException) {
                    // Core names the ends a join can come to — an expired code,
                    // the other device not running it, the other device ending
                    // it — and each one is its own line for the device locale.
                    logger.error("pairing join failed", e)
                    error = context.localizedLine(e) ?: e.toString()
                } catch (e: Exception) {
                    logger.error("pairing join failed", e)
                    error = e.toString()
                } finally {
                    progressDelivery.close()
                    joiningFingerprint = null
                    joinProgress = null
                    if (flow === started) flow = null
                }
            }
        started = JoinFlow(launched)
        flow = started
        launched.start()
    }

    suspend fun resumePending(
        oauthLinking: OAuthLinker?,
        oauthLinkingError: String?,
    ): Boolean {
        val pending =
            try {
                withContext(Dispatchers.IO) { pendingDevicePairingJoin() }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.toString()
                null
            }
        if (pending == null) return false

        pairingCode = pending.pairingCode
        decodedOffer = Result.success(pending.offer)
        joiningFingerprint = pending.fingerprint
        hasPendingPairing = true
        error = null
        oauthTokenJson = null

        var canJoin = true
        if (
            pending.offer.needsOauth &&
            pending.phase != BridgeDevicePairingPhase.LIBRARY_INSTALLATION_PENDING
        ) {
            isAuthorizing = true
            try {
                oauthTokenJson =
                    resolveOauthToken(
                        oauthLinking,
                        oauthLinkingError,
                        context,
                        pending.offer.cloudProvider,
                    )
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: context.getString(R.string.onboarding_oauth_unconfigured)
                canJoin = false
            } finally {
                isAuthorizing = false
            }
        }
        if (canJoin) join()
        return true
    }

    fun abandonAndReset(onAbandoned: () -> Unit) {
        authorizationJob?.cancel()
        val activeFlow = flow
        activeFlow?.cancel()
        flow = null
        scope.launch {
            try {
                activeFlow?.job?.join()
                withContext(Dispatchers.IO) { abandonPendingDevicePairingJoin() }
                reset()
                onAbandoned()
            } catch (e: Exception) {
                error = e.toString()
            }
        }
    }
}

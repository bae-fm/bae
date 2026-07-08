package fm.bae.app.ui

import fm.bae.app.BaeLogger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val TAG = "bae.DisconnectSyncFlow"
private val logger = BaeLogger(TAG)

/**
 * Confirmation-dialog visibility and inline-error state for disconnecting the
 * cloud provider from this device.
 */
internal data class DisconnectSyncState(
    /** Whether the confirmation dialog is shown. */
    val confirming: Boolean = false,
    /**
     * bae-core's pre-formatted at-risk sentence to append to the confirmation
     * body, or null when nothing is at risk (or the check itself failed).
     */
    val extraWarning: String? = null,
    /** Inline error line in the Sync section, or null. */
    val error: String? = null,
)

/**
 * The confirmation body: a localized [base] sentence with bae-core's
 * pre-formatted at-risk sentence appended verbatim after a single space when
 * present. The at-risk sentence is the one string bae-core hands over already
 * rendered (a deliberate exception to keeping locale out of the bridge), so it
 * is not re-localized here.
 */
internal fun disconnectConfirmMessage(
    base: String,
    extraWarning: String?,
): String = if (extraWarning.isNullOrEmpty()) base else "$base $extraWarning"

/**
 * Drives the disconnect confirmation and execution with injected bridge calls so
 * the screen supplies the live handle and tests supply stubs.
 *
 * Two invariants:
 * - the disconnect runs off the main thread, because bae-core joins the
 *   sync-loop thread and can block for the remainder of an in-flight cycle;
 * - a failed at-risk check still opens the confirmation, surfacing the failure
 *   inline so the user sees the data-loss check didn't run yet can still proceed.
 */
internal class DisconnectSyncFlow(
    private val scope: CoroutineScope,
    private val warningMessage: suspend () -> String?,
    private val disconnect: () -> Unit,
    private val warningFailedLine: (Throwable) -> String,
    private val disconnectFailedLine: (Throwable) -> String,
    private val ioDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private val _state = MutableStateFlow(DisconnectSyncState())
    val state: StateFlow<DisconnectSyncState> = _state.asStateFlow()

    private var warningJob: Job? = null

    /**
     * Query the at-risk warning, then open the confirmation. On query failure,
     * surface the error inline and still open the confirmation so the user can
     * proceed or cancel. A superseding call cancels the prior query.
     */
    fun promptDisconnect() {
        _state.update { it.copy(error = null) }
        warningJob?.cancel()
        warningJob =
            scope.launch {
                _state.value =
                    try {
                        DisconnectSyncState(confirming = true, extraWarning = warningMessage())
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Throwable) {
                        logger.error("Failed to compute disconnect warning", e)
                        DisconnectSyncState(confirming = true, error = warningFailedLine(e))
                    }
            }
    }

    fun dismissConfirm() {
        _state.update { it.copy(confirming = false) }
    }

    /**
     * Disconnect off the main thread. On success the sync config is cleared by
     * core and the whole connected section falls away via config invalidation;
     * on failure the error shows inline and nothing durable changed.
     */
    suspend fun confirm() {
        _state.value =
            try {
                withContext(ioDispatcher) { disconnect() }
                DisconnectSyncState()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                logger.error("Failed to disconnect cloud provider", e)
                DisconnectSyncState(error = disconnectFailedLine(e))
            }
    }
}

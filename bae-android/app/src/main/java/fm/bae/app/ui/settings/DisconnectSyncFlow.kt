package fm.bae.app.ui.settings

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
     * The localized at-risk sentence to append to the confirmation body, or null
     * when nothing is at risk (or the check itself failed).
     */
    val extraWarning: String? = null,
    /** Inline error line in the Sync section, or null. */
    val error: String? = null,
)

/**
 * The confirmation body: a localized [base] sentence with the at-risk sentence
 * appended after a single space when present. bae-core supplies the count; the
 * sentence itself is resolved here from `core.sync.cloud_only_releases`, with
 * this locale's plural rules.
 */
internal fun disconnectConfirmMessage(
    base: String,
    extraWarning: String?,
): String = if (extraWarning.isNullOrEmpty()) base else "$base $extraWarning"

/**
 * The localized lines the disconnect flow shows: the at-risk warning for a
 * cloud-only release count, and the failure lines for a warning check or a
 * disconnect that errored. The screen supplies live resource-backed formatters;
 * tests supply stubs.
 */
internal data class DisconnectStrings(
    val atRiskLine: (ULong) -> String,
    /** Null when core says the failure has no line to show — a cancellation. */
    val warningFailedLine: (Throwable) -> String?,
    /** Null when core says the failure has no line to show — a cancellation. */
    val disconnectFailedLine: (Throwable) -> String?,
)

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
    private val cloudOnlyReleaseCount: suspend () -> ULong,
    private val disconnect: () -> Unit,
    private val strings: DisconnectStrings,
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
                        val count = cloudOnlyReleaseCount()
                        DisconnectSyncState(
                            confirming = true,
                            extraWarning = if (count > 0uL) strings.atRiskLine(count) else null,
                        )
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Throwable) {
                        logger.error("Failed to compute disconnect warning", e)
                        DisconnectSyncState(confirming = true, error = strings.warningFailedLine(e))
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
                DisconnectSyncState(error = strings.disconnectFailedLine(e))
            }
    }
}

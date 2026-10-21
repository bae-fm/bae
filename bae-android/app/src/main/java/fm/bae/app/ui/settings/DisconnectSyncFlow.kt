package fm.bae.app.ui.settings

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

private const val TAG = "bae.DisconnectSyncFlow"
private val logger = BaeLogger(TAG)

/**
 * What the disconnect confirmation says this device would lose: nothing only in
 * the cloud, a count of releases that are, or that the check itself failed.
 */
internal sealed interface DisconnectRisk {
    /** Every release this device can play is also stored somewhere it keeps. */
    data object NothingCloudOnly : DisconnectRisk

    /** The localized at-risk sentence for the releases that exist only in the cloud. */
    data class CloudOnly(
        val line: String,
    ) : DisconnectRisk

    /** The count failed, so the confirmation can't say what would be lost. */
    data class CheckFailed(
        val error: DisplayError,
    ) : DisconnectRisk
}

/**
 * The open confirmation, if any, and the inline error of a disconnect that
 * failed.
 */
internal data class DisconnectSyncState(
    /** The open confirmation's risk, or null when the dialog is closed. */
    val confirming: DisconnectRisk? = null,
    /** A failed disconnect, shown inline in the Sync section, or null. */
    val error: DisplayError? = null,
)

/**
 * The confirmation body: a localized [base] sentence with the at-risk sentence
 * appended after a single space. bae-core supplies the count; the sentence
 * itself is resolved from `core.sync.cloud_only_releases`, with this locale's
 * plural rules.
 */
internal fun disconnectConfirmMessage(
    base: String,
    atRisk: DisconnectRisk.CloudOnly,
): String = "$base ${atRisk.line}"

/**
 * The localized text the disconnect flow shows: the at-risk warning for a
 * cloud-only release count, and the failures of a warning check or a
 * disconnect that errored. The screen supplies live resource-backed formatters;
 * tests supply stubs.
 */
internal data class DisconnectStrings(
    val atRiskLine: (ULong) -> String,
    /** Null when core says the failure has nothing to show — a cancellation. */
    val warningFailed: (Throwable) -> DisplayError?,
    /** Null when core says the failure has nothing to show — a cancellation. */
    val disconnectFailed: (Throwable) -> DisplayError?,
)

/**
 * Drives the disconnect confirmation and execution with injected bridge calls so
 * the screen supplies the live handle and tests supply stubs.
 *
 * A failed at-risk check still opens the confirmation, and the confirmation
 * says the check failed, so the person knows nothing told them what they'd lose
 * yet can still proceed.
 */
internal class DisconnectSyncFlow(
    private val scope: CoroutineScope,
    private val cloudOnlyReleaseCount: suspend () -> ULong,
    private val disconnect: suspend () -> Unit,
    private val strings: DisconnectStrings,
) {
    private val _state = MutableStateFlow(DisconnectSyncState())
    val state: StateFlow<DisconnectSyncState> = _state.asStateFlow()

    private var warningJob: Job? = null

    /**
     * Query the at-risk count, then open the confirmation with what it found or
     * with the check's failure. A superseding call cancels the prior query; a
     * check core reports as cancelled opens nothing.
     */
    fun promptDisconnect() {
        _state.update { it.copy(error = null) }
        warningJob?.cancel()
        warningJob =
            scope.launch {
                val risk =
                    try {
                        val count = cloudOnlyReleaseCount()
                        if (count > 0uL) {
                            DisconnectRisk.CloudOnly(strings.atRiskLine(count))
                        } else {
                            DisconnectRisk.NothingCloudOnly
                        }
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: Throwable) {
                        logger.error("Failed to compute disconnect warning", e)
                        val error = strings.warningFailed(e)
                        if (error == null) {
                            logger.info("Disconnect warning check was cancelled; not opening the confirmation")
                            return@launch
                        }
                        DisconnectRisk.CheckFailed(error)
                    }
                _state.value = DisconnectSyncState(confirming = risk)
            }
    }

    fun dismissConfirm() {
        _state.update { it.copy(confirming = null) }
    }

    /**
     * On success the sync config is cleared by core and the whole connected
     * section falls away via the config subscription; on failure the error
     * shows inline and nothing durable changed.
     */
    suspend fun confirm() {
        _state.value =
            try {
                disconnect()
                DisconnectSyncState()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Throwable) {
                logger.error("Failed to disconnect cloud provider", e)
                DisconnectSyncState(error = strings.disconnectFailed(e))
            }
    }
}

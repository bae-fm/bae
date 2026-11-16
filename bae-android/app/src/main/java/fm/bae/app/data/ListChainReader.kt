package fm.bae.app.data

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount

/**
 * One open list: its chain of window reads (see [ListChain]) and its count, each a live read, and the jump that
 * starts the chain over at a row far from the chain's rows. The reader calls [onChange] after each delivery of the
 * count or of a window, and [onFailure] when any of its reads fails. Its owner decides whether a call still counts,
 * since a value can arrive after the owner moved on to other reads.
 */
internal class ListChainReader<Row>(
    private val scope: CoroutineScope,
    private val reads: ListReads<Row>,
    private val onChange: () -> Unit,
    private val onFailure: (BridgeException) -> Unit,
) {
    val chain = ListChain(scope, reads, BROWSER_PAGE_SIZE, onChange, onFailure)

    /** The list's count from its last value; null until the count delivers. */
    var count by mutableStateOf<BridgeListCount?>(null)
        private set

    /** Whether the count and the chain's first window have both returned since the chain last started. */
    val answered: Boolean get() = count != null && chain.hasAnswered

    private var countDeliveries: ScreenSubscription<BridgeListCount>? = null
    private var jump: Job? = null
    private var stopped = false

    /** Open the count and start the chain at the list's top. */
    fun start() {
        countDeliveries =
            ScreenSubscription(
                reads.count(),
                scope,
                onValue = {
                    count = it
                    onChange()
                },
                onError = onFailure,
            )
        chain.restart(null, 0)
    }

    /**
     * Keep the chain around the rows from [first] to [last] on screen: it grows towards them, or starts over at the
     * key the list has at [first] when they are far from its rows. The reader does nothing while it reads a jump's
     * key, or after [stop].
     */
    fun show(
        first: Int,
        last: Int,
    ) {
        if (stopped || first > last || jump != null) return
        chain.visible = first..last
        if (chain.isEmpty || !chain.reaches(first) || !chain.reaches(last)) {
            jumpTo(first)
        } else {
            chain.reconcile()
        }
    }

    /** End every read. The rows the chain last delivered stay in [ListChain.placed]. */
    fun stop() {
        stopped = true
        jump?.cancel()
        jump = null
        chain.stop()
        countDeliveries?.stop()
        countDeliveries = null
    }

    /** Start the chain over at the row the list has at [position], a plain read of its key, not a live one. */
    private fun jumpTo(position: Int) {
        chain.stop()
        jump =
            scope.launch {
                val key =
                    try {
                        reads.keyAt(position.toULong())
                    } catch (error: BridgeException) {
                        jump = null
                        onFailure(error)
                        return@launch
                    }
                if (stopped) return@launch
                jump = null
                // A position past the list's end has the last row's key, which sits at the last position.
                val known = count?.let { maxOf(it.rows.toInt() - 1, 0) } ?: position
                chain.restart(key, minOf(position, known))
            }
    }
}

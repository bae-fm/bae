package fm.bae.app.ui.settings

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.ScreenSubscription
import kotlinx.coroutines.CoroutineScope
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.KeyedRead")

/** A value a live read returned, with the key the read asked for. */
internal data class Shown<Key, Value>(
    val key: Key,
    val value: Value,
)

/** The value when the read returned it for [key]; null when it returned it for another key, or returned nothing yet. */
internal fun <Key, Value> Shown<Key, Value>?.at(key: Key): Value? = this?.takeIf { it.key == key }?.value

/**
 * One live read that a screen asks for by a key, such as a member id or a place, while the screen shows it. Asking
 * for another key replaces the read, and [shown] keeps the last value until the new read returns its first, so the
 * screen compares the key when it must not draw another key's value. [subscribe] starts the read of a key; a read
 * that can't start or that fails goes to [onError], and [what] names the read in the log.
 */
internal class KeyedRead<Key, Value>(
    private val scope: CoroutineScope,
    private val what: String,
    private val subscribe: (Key) -> PulledSubscription<Value>,
    private val onError: (BridgeException) -> Unit,
) {
    /**
     * The value the read returned last, with the key it asked for; null before the first read returns, and after
     * [hide].
     */
    var shown by mutableStateOf<Shown<Key, Value>?>(null)
        private set

    /** The key the running read asks for, while one runs. */
    private var key: Key? = null
    private var subscription: ScreenSubscription<Value>? = null

    /** Which read a callback belongs to; one from a replaced read is dropped. */
    private var generation = 0

    /** Read [key], replacing the read of another key. Asking again for the key that runs changes nothing. */
    fun show(key: Key) {
        if (subscription != null && this.key == key) return
        stop()
        this.key = key
        val generation = generation
        val read =
            try {
                subscribe(key)
            } catch (e: BridgeException) {
                logger.error("reading $what failed", e)
                onError(e)
                return
            }
        subscription =
            ScreenSubscription(
                read,
                scope,
                onValue = { value -> if (generation == this.generation) shown = Shown(key, value) },
                onError = { error ->
                    logger.error("reading $what failed", error)
                    if (generation == this.generation) onError(error)
                },
            )
    }

    /** End the read and forget what it returned, once no screen shows it. */
    fun hide() {
        stop()
        key = null
        shown = null
    }

    private fun stop() {
        generation++
        subscription?.stop()
        subscription = null
    }
}

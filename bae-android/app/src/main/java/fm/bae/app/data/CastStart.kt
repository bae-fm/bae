package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.CastStart")

/**
 * The cast the person started last, while it connects. There is one for the open library, not one per
 * place that lists receivers: the picker and the Casting page show the same start, and closing the one
 * that began it leaves it connecting. A newer start calls off the one before it. A start that connects
 * shows as [started]; a place that began one closes itself when [didStart] says it is the [Attempt] it
 * was given, so a place opened later never closes for a start it did not ask for.
 *
 * A start that fails says why where the person is: in the picker, which is a sheet that hides a snackbar,
 * while it is open ([failure]); otherwise through [tell]. [failureOf] words a failure, and is null for one
 * core gives no line, such as a start called off by stopping casting, turning it off, or another cast.
 */
class CastStart(
    private val scope: CoroutineScope,
    private val castTo: suspend (String) -> Unit,
    private val failureOf: (BridgeException) -> DisplayError?,
    private val pickerOpen: () -> Boolean,
    private val tell: (DisplayError) -> Unit,
) {
    private val _connecting = MutableStateFlow<BridgeCastReceiver?>(null)

    /** The receiver a cast is connecting to, while it does. */
    val connecting: StateFlow<BridgeCastReceiver?> = _connecting.asStateFlow()

    private val _failure = MutableStateFlow<DisplayError?>(null)

    /** Why the last start failed while the picker was open, until the next start or the picker closes. */
    val failure: StateFlow<DisplayError?> = _failure.asStateFlow()

    private val _started = MutableStateFlow<Attempt?>(null)

    /** The latest start that reached playback on its receiver. */
    val started: StateFlow<Attempt?> = _started.asStateFlow()

    private var attempts = 0
    private var job: Job? = null

    /** One call to [cast]. [started] equals it once that start connects. */
    class Attempt internal constructor(
        private val number: Int,
    ) {
        override fun equals(other: Any?) = other is Attempt && other.number == number

        override fun hashCode() = number
    }

    /**
     * Whether [attempt], what a place was given when it asked, is the start that connected. A place that
     * asked nothing, or asked an earlier start that a later one replaced, gets false.
     */
    fun didStart(attempt: Attempt?): Boolean = attempt != null && attempt == _started.value

    /**
     * Switch playback to [receiver], calling off the start an earlier call began. Returns the attempt
     * [started] becomes once playback is on it.
     */
    fun cast(receiver: BridgeCastReceiver): Attempt {
        job?.cancel()
        val attempt = Attempt(++attempts)
        _failure.value = null
        _connecting.value = receiver
        job =
            scope.launch {
                try {
                    castTo(receiver.id)
                    _connecting.value = null
                    _started.value = attempt
                } catch (e: BridgeException) {
                    logger.error("Failed to cast to ${receiver.id}", e)
                    _connecting.value = null
                    failureOf(e)?.let { shown ->
                        if (pickerOpen()) _failure.value = shown else tell(shown)
                    }
                }
            }
        return attempt
    }

    /** The picker closed: what it showed of a failure goes with it. */
    fun clearFailure() {
        _failure.value = null
    }
}

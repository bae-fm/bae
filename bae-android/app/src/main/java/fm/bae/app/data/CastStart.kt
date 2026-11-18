package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AirPlayCodePromptCallback
import uniffi.bae_bridge.BridgeAirPlayCodeRequest
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
 * while it is open ([failure]); otherwise through [tell]. [failureOf] words a failure and names the
 * receiver that refused it, and is null for one core gives no line, such as a start called off by
 * stopping casting, turning it off, or another cast.
 *
 * An AirPlay receiver that wants a PIN or a password asks the person through [codeSheet], which the app
 * shows as a dialog over whatever is on screen while it is set: the person's code goes back with [pair],
 * and [cancelCode] calls the cast off.
 */
class CastStart(
    private val scope: CoroutineScope,
    private val castTo: suspend (String, AirPlayCodePromptCallback) -> Unit,
    private val failureOf: (BridgeException) -> Failure?,
    private val pickerOpen: () -> Boolean,
    private val tell: (DisplayError) -> Unit,
) {
    private val _connecting = MutableStateFlow<BridgeCastReceiver?>(null)

    /** The receiver a cast is connecting to, while it does. */
    val connecting: StateFlow<BridgeCastReceiver?> = _connecting.asStateFlow()

    private val _failure = MutableStateFlow<Failure?>(null)

    /** Why the last start failed while the picker was open, until the next start or the picker closes. */
    val failure: StateFlow<Failure?> = _failure.asStateFlow()

    private val _started = MutableStateFlow<Attempt?>(null)

    /** The latest start that reached playback on its receiver. */
    val started: StateFlow<Attempt?> = _started.asStateFlow()

    private val _codeSheet = MutableStateFlow<CodeSheet?>(null)

    /** The dialog that asks for the code the receiver of the cast connecting wants, while it is open. */
    val codeSheet: StateFlow<CodeSheet?> = _codeSheet.asStateFlow()

    /** What answers the open dialog's request, once the person presses Pair. */
    private var codeAnswer: CompletableDeferred<String>? = null

    private var attempts = 0
    private var job: Job? = null

    /**
     * Why a start failed, and the receiver that refused it when core says a receiver refused: the picker
     * titles its notice with that receiver's name.
     */
    data class Failure(
        val error: DisplayError,
        val refusedBy: String?,
    )

    /**
     * What the open dialog asks: the receiver, core's request, and whether the person's code is out, so the
     * dialog shows Pairing… until core asks again or the cast ends.
     */
    data class CodeSheet(
        val receiver: BridgeCastReceiver,
        val request: BridgeAirPlayCodeRequest,
        val pairing: Boolean,
        internal val attempt: Int,
    )

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
        closeCodeSheet()
        val number = ++attempts
        val attempt = Attempt(number)
        _failure.value = null
        _connecting.value = receiver
        job =
            scope.launch {
                try {
                    castTo(receiver.id, CodePrompt(receiver, number))
                    closeCodeSheet(number)
                    _connecting.value = null
                    _started.value = attempt
                } catch (e: BridgeException) {
                    logger.error("Failed to cast to ${receiver.id}", e)
                    closeCodeSheet(number)
                    _connecting.value = null
                    failureOf(e)?.let { shown ->
                        if (pickerOpen()) _failure.value = shown else tell(shown.error)
                    }
                }
            }
        return attempt
    }

    /** Hand core the code the person entered in the open dialog, which shows Pairing… until core asks again. */
    fun pair(code: String) {
        val sheet = _codeSheet.value ?: return
        val answer = codeAnswer ?: return
        codeAnswer = null
        _codeSheet.value = sheet.copy(pairing = true)
        answer.complete(code)
    }

    /** The person cancelled the open dialog: close it and call off the cast it asks for, which casts nothing. */
    fun cancelCode() {
        if (_codeSheet.value == null) return
        job?.cancel()
        job = null
        _connecting.value = null
        closeCodeSheet()
    }

    /**
     * Show [request] for the cast [attempt] and wait for Pair. A cast that ends while this waits cancels this
     * call, which closes the dialog.
     */
    private suspend fun askCode(
        request: BridgeAirPlayCodeRequest,
        receiver: BridgeCastReceiver,
        attempt: Int,
    ): String =
        withContext(Dispatchers.Main.immediate) {
            // A cast another start replaced asks nothing more; core has dropped what waits for this answer.
            if (attempt != attempts) return@withContext ""
            val answer = CompletableDeferred<String>()
            codeAnswer?.complete("")
            codeAnswer = answer
            _codeSheet.value = CodeSheet(receiver, request, pairing = false, attempt = attempt)
            try {
                answer.await()
            } finally {
                if (codeAnswer === answer) closeCodeSheet(attempt)
            }
        }

    /** Close the dialog, or only the dialog of [attempt], answering a request still open with nothing. */
    private fun closeCodeSheet(attempt: Int? = null) {
        if (attempt != null && _codeSheet.value?.attempt != attempt) return
        _codeSheet.value = null
        codeAnswer?.complete("")
        codeAnswer = null
    }

    /** The prompt one cast asks for its receiver's code through: the start's dialog. */
    private inner class CodePrompt(
        private val receiver: BridgeCastReceiver,
        private val attempt: Int,
    ) : AirPlayCodePromptCallback {
        override suspend fun code(request: BridgeAirPlayCodeRequest): String = askCode(request, receiver, attempt)
    }

    /** The picker closed: what it showed of a failure goes with it. */
    fun clearFailure() {
        _failure.value = null
    }
}

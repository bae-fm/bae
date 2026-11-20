package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AirPlayCodePromptCallback
import uniffi.bae_bridge.BridgeAirPlayCodeRequest
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.CastStart")

/**
 * The cast the person started last, while it connects. There is one for the open library, not one per
 * place that lists receivers: the picker and the Playback settings show the same start, and closing the one
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

    /** The open dialog, while one is; [codeSheet] shows it. */
    private var codeState: CodeState? = null
        set(value) {
            field = value
            _codeSheet.value = value?.sheet
        }

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
    @ConsistentCopyVisibility
    data class CodeSheet internal constructor(
        private val asked: Asked,
        val pairing: Boolean,
    ) {
        val receiver: BridgeCastReceiver get() = asked.receiver
        val request: BridgeAirPlayCodeRequest get() = asked.request

        /** The cast whose dialog this is, which keeps one dialog open across its requests. */
        internal val attempt: Int get() = asked.attempt
    }

    /** What a receiver of the cast [attempt] asked for. */
    internal data class Asked(
        val receiver: BridgeCastReceiver,
        val request: BridgeAirPlayCodeRequest,
        val attempt: Int,
    )

    /** Where the open dialog is. */
    private sealed interface CodeState {
        val asked: Asked

        /** The dialog waits for the person's code, which [answer] takes to core. */
        class Asking(
            override val asked: Asked,
            val answer: CompletableDeferred<String>,
        ) : CodeState

        /** The person's code is out, and the dialog shows Pairing… until core asks again or the cast ends. */
        class Pairing(
            override val asked: Asked,
        ) : CodeState

        val sheet: CodeSheet
            get() = CodeSheet(asked, pairing = this is Pairing)
    }

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
        // Core holds the prompt for the whole cast, so the prompt holds only the sending end of the channel this
        // start reads its requests from. A request the start never shows gets nothing back.
        val asks = Channel<CodeAsk>(Channel.UNLIMITED) { (it as? CodeAsk.Ask)?.answer?.complete("") }
        val showing = scope.launch { for (ask in asks) take(ask, receiver, number) }
        job =
            scope.launch {
                try {
                    castTo(receiver.id, CodePrompt(asks))
                    _connecting.value = null
                    _started.value = attempt
                } catch (e: BridgeException) {
                    logger.error("Failed to cast to ${receiver.id}", e)
                    _connecting.value = null
                    failureOf(e)?.let { shown ->
                        if (pickerOpen()) _failure.value = shown else tell(shown.error)
                    }
                } finally {
                    // However the cast ended, its dialog closes with it.
                    asks.close()
                    showing.cancel()
                    closeCodeSheet(number)
                }
            }
        return attempt
    }

    /** Hand core the code the person entered in the open dialog, which shows Pairing… until core asks again. */
    fun pair(code: String) {
        val asking = codeState as? CodeState.Asking ?: return
        codeState = CodeState.Pairing(asking.asked)
        asking.answer.complete(code)
    }

    /** The person cancelled the open dialog: close it and call off the cast it asks for, which casts nothing. */
    fun cancelCode() {
        if (codeState == null) return
        job?.cancel()
        job = null
        _connecting.value = null
        closeCodeSheet()
    }

    /** Show what the prompt of the cast [attempt] sent. */
    private fun take(
        ask: CodeAsk,
        receiver: BridgeCastReceiver,
        attempt: Int,
    ) {
        when (ask) {
            is CodeAsk.Ask -> {
                // A cast another start replaced asks nothing more.
                if (attempt != attempts) {
                    ask.answer.complete("")
                    return
                }
                (codeState as? CodeState.Asking)?.answer?.complete("")
                codeState = CodeState.Asking(Asked(receiver, ask.request, attempt), ask.answer)
            }

            CodeAsk.Withdrawn -> {
                closeCodeSheet(attempt)
            }
        }
    }

    /** Close the dialog, or only the dialog of [attempt], answering a request still open with nothing. */
    private fun closeCodeSheet(attempt: Int? = null) {
        val open = codeState ?: return
        if (attempt != null && open.asked.attempt != attempt) return
        codeState = null
        (open as? CodeState.Asking)?.answer?.complete("")
    }

    /** The picker closed: what it showed of a failure goes with it. */
    fun clearFailure() {
        _failure.value = null
    }
}

/** What a cast's prompt sends its start: a request to show, with what takes the person's code back to core, or word
 *  that core stopped waiting. */
private sealed interface CodeAsk {
    class Ask(
        val request: BridgeAirPlayCodeRequest,
        val answer: CompletableDeferred<String>,
    ) : CodeAsk

    data object Withdrawn : CodeAsk
}

/** The prompt one cast asks for its receiver's code through: the sending end of the channel its start shows each
 *  request from. */
private class CodePrompt(
    private val asks: SendChannel<CodeAsk>,
) : AirPlayCodePromptCallback {
    override suspend fun code(request: BridgeAirPlayCodeRequest): String {
        val answer = CompletableDeferred<String>()
        // A start that stopped reading asks nothing more.
        if (asks.trySend(CodeAsk.Ask(request, answer)).isFailure) return ""
        try {
            return answer.await()
        } finally {
            if (!answer.isCompleted) asks.trySend(CodeAsk.Withdrawn)
        }
    }
}

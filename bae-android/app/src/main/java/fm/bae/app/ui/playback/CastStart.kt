package fm.bae.app.ui.playback

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.displayError
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.CastStart")

/**
 * Starting a cast to a receiver, shared by the cast picker and the Casting page: the receiver it is
 * connecting to while it does, the job, a newer start cancelling an older one, and why a start failed.
 * [failureOf] words a failure, and is null for one core gives no line, such as a start called off.
 */
internal class CastStart(
    private val scope: CoroutineScope,
    private val castTo: suspend (String) -> Unit,
    private val failureOf: (BridgeException) -> DisplayError?,
) {
    /** The receiver a cast is connecting to, while it does. */
    var connecting by mutableStateOf<BridgeCastReceiver?>(null)
        private set

    /** Why the last start failed, until the next one begins. */
    var failure by mutableStateOf<DisplayError?>(null)
        private set

    private var job: Job? = null

    /**
     * Switch playback to [receiver], calling off the start an earlier call began.
     * [onStarted] runs once playback is on it.
     */
    fun cast(
        receiver: BridgeCastReceiver,
        onStarted: () -> Unit,
    ) {
        job?.cancel()
        failure = null
        connecting = receiver
        job =
            scope.launch {
                try {
                    castTo(receiver.id)
                    connecting = null
                    onStarted()
                } catch (e: BridgeException) {
                    logger.error("Failed to cast to ${receiver.id}", e)
                    connecting = null
                    failure = failureOf(e)
                }
            }
    }

    /** Stop waiting for a start still connecting: its place went away. */
    fun cancel() {
        job?.cancel()
        job = null
        connecting = null
    }
}

/** A [CastStart] over [session]'s cast, called off when the place that remembers it leaves composition. */
@Composable
internal fun rememberCastStart(session: OpenLibrary): CastStart {
    val scope = rememberScreenScope()
    val context: Context = LocalContext.current
    val start =
        remember(session, scope, context) {
            CastStart(scope, session.cast::castTo) { context.displayError(it) }
        }
    DisposableEffect(start) { onDispose { start.cancel() } }
    return start
}

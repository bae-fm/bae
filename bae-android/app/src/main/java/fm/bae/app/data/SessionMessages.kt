package fm.bae.app.data

import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.performBridgeAction
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

private val logger = BaeLogger("bae.SessionMessages")

/**
 * One snackbar's worth: what failed, and, for something the person did, the
 * Retry that runs it again. Without [retry] the snackbar only dismisses.
 */
internal data class SessionMessage(
    val error: DisplayError,
    val retry: (() -> Unit)?,
)

/**
 * The open library's snackbar messages, shown on whatever screen of the
 * library is showing: failures of what the person did, with Retry, and
 * failures core reports that have no screen of their own, to dismiss.
 * Messages wait here until the snackbar host takes them, so one sent while no
 * screen shows is shown once one does.
 */
class SessionMessages internal constructor(
    /** The session's scope, so an action and its Retry outlive the screen they started on. */
    private val scope: CoroutineScope,
    private val errors: ErrorDisplays,
) {
    private val channel = Channel<SessionMessage>(Channel.UNLIMITED)

    internal val messages: Flow<SessionMessage> = channel.receiveAsFlow()

    /**
     * A failure core reported with no screen of its own, shown until
     * dismissed. Null is core saying there is nothing to show — a
     * cancellation — so nothing shows.
     */
    fun show(error: DisplayError?) {
        if (error == null) {
            logger.debug("a failure with nothing to show (a cancellation) was not shown")
            return
        }
        channel.trySend(SessionMessage(error, retry = null))
    }

    /**
     * Run [action], something the person did, in the session. Its failure
     * shows as a snackbar whose Retry runs it again.
     */
    fun perform(
        operation: String,
        action: suspend () -> Unit,
    ) {
        scope.launch {
            performBridgeAction(logger, operation, errors, { shown ->
                if (shown == null) {
                    logger.debug("$operation was cancelled; nothing to show")
                } else {
                    channel.trySend(SessionMessage(shown, retry = { perform(operation, action) }))
                }
            }, action)
        }
    }
}

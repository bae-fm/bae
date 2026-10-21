package fm.bae.app

import androidx.compose.runtime.staticCompositionLocalOf
import io.sentry.Sentry
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow

private val logger = BaeLogger("bae.BugReports")

/**
 * Bugs — code that broke its own guarantee — reported without crashing: to
 * the crash reporter as a non-fatal event, to the log, and to the person, who
 * is told "Something went wrong" by the app's snackbar. Expected failures are
 * not bugs and never come here; they are typed errors their screen shows.
 * One instance for the process, held by [BaeApp].
 */
class BugReports {
    private val channel = Channel<Unit>(Channel.UNLIMITED)

    /** One element per bug, for the snackbar that tells the person. */
    val reported: Flow<Unit> = channel.receiveAsFlow()

    fun report(
        operation: String,
        error: Throwable,
    ) {
        logger.error("bug: $operation failed", error)
        Sentry.captureException(error) { scope -> scope.setExtra("operation", operation) }
        channel.trySend(Unit)
    }

    /**
     * Installed on every coroutine scope of the open library's session and
     * screens, so a failure nothing caught is reported as a bug rather than
     * crashing or vanishing. The coroutine's [CoroutineName] names the
     * operation when it has one.
     */
    val handler: CoroutineExceptionHandler =
        CoroutineExceptionHandler { context, error ->
            report(context[CoroutineName]?.name ?: "a coroutine", error)
        }
}

/**
 * The bug handler screen scopes install, provided at the app root. No default:
 * a screen scope with no handler would crash on a bug, so a missing provider
 * fails loudly instead.
 */
val LocalBugHandler =
    staticCompositionLocalOf<CoroutineExceptionHandler> {
        error("LocalBugHandler not provided")
    }

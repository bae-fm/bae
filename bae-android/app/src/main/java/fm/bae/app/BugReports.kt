package fm.bae.app

import androidx.compose.runtime.staticCompositionLocalOf
import io.sentry.Sentry
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import uniffi.bae_bridge.BridgeBugReporter

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

    /** Report a bug and tell the person by the app's snackbar. */
    fun report(
        operation: String,
        error: Throwable,
    ) {
        reportShownInPlace(operation, error)
        channel.trySend(Unit)
    }

    /** Report a bug whose "Something went wrong" the caller shows itself, where the person is looking. */
    fun reportShownInPlace(
        operation: String,
        error: Throwable,
    ) {
        logger.error("bug: $operation failed", error)
        Sentry.captureException(error) { scope -> scope.setExtra("operation", operation) }
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
 * The app's bug reports, provided at the app root: screen scopes install its
 * handler, and a screen that finds a broken guarantee reports it here. No
 * default: a screen scope with no handler would crash on a bug, so a missing
 * provider fails loudly instead.
 */
val LocalBugReports =
    staticCompositionLocalOf<BugReports> {
        error("LocalBugReports not provided")
    }

/**
 * The crash reporter core reports its bugs to — code that broke its own
 * guarantee: each one a non-fatal Sentry event carrying the operation and the
 * error's full text. Core has already logged it. Without Sentry initialized
 * (the baeium edition, a test) the event goes nowhere. Core calls it from
 * whatever thread found the bug; Sentry's capture is thread-safe.
 */
object CoreBugReporter : BridgeBugReporter {
    override fun report(
        operation: String,
        detail: String,
    ) {
        Sentry.captureException(CoreBug(operation, detail)) { scope -> scope.setExtra("operation", operation) }
    }
}

/** A bug core reported, as the exception Sentry records. */
class CoreBug(
    operation: String,
    detail: String,
) : Exception("$operation: $detail")

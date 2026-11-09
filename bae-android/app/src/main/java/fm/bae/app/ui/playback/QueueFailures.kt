package fm.bae.app.ui.playback

import android.content.Context
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.displayError
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.TitledErrorNotice
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.QueueFailures")

/**
 * The queue sheet's failures as the person sees them: the last queue command that failed, shown at
 * the top of the sheet, since the library's banner sits under it.
 */
internal class QueueFailures(
    private val context: Context,
    private val bugs: BugReports,
) {
    var command by mutableStateOf<DisplayError?>(null)
        private set

    /** Take the shown command failure away. */
    fun dismiss() {
        command = null
    }

    /**
     * Run a queue command that answers, as [run] does, and hand back its answer; null when it failed,
     * which is shown.
     */
    suspend fun <T : Any> answer(
        operation: String,
        action: suspend () -> T,
    ): T? =
        try {
            action().also { command = null }
        } catch (e: CancellationException) {
            throw e
        } catch (e: BridgeException) {
            logger.error("$operation failed", e)
            command = context.displayError(e)
            null
        } catch (e: Exception) {
            bugs.reportShownInPlace(operation, e)
            command = DisplayError(context.coreString("core.error.category.internal"))
            null
        }

    /**
     * Run a queue command; a failure is logged and shown, a success clears the
     * last one. A queue command fails only with a typed bridge error, so any
     * other exception is a bug: reported, and shown as "Something went wrong".
     */
    fun run(
        operation: String,
        action: () -> Unit,
    ) {
        command =
            try {
                action()
                null
            } catch (e: BridgeException) {
                logger.error("$operation failed", e)
                context.displayError(e)
            } catch (e: Exception) {
                bugs.reportShownInPlace(operation, e)
                DisplayError(context.coreString("core.error.category.internal"))
            }
    }
}

@Composable
internal fun rememberQueueFailures(): QueueFailures {
    val context = LocalContext.current
    val bugs = LocalBugReports.current
    return remember { QueueFailures(context, bugs) }
}

/** The last queue command that failed, above the lanes; nothing while none has. */
@Composable
internal fun QueueCommandFailure(failures: QueueFailures) {
    val failure = failures.command ?: return
    ErrorNotice(
        error = failure,
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge),
        look = NoticeLook.QUIET,
        onDismiss = failures::dismiss,
    )
}

/**
 * Why the read of the context's tail stopped, centred under its lane, with Details and Retry, which
 * opens it again; nothing while it reads. Rows past the first window stay placeholders until it does.
 */
@Composable
internal fun UpcomingFailureNotice(session: OpenLibrary) {
    val upcoming = session.playback.upcoming
    val failure by upcoming.failure.collectAsState()
    val context = LocalContext.current
    val shown = failure?.let { context.displayError(it) } ?: return
    TitledErrorNotice(
        title = context.coreString("ui.queue.upcoming_failed"),
        advice = null,
        error = shown,
        modifier = Modifier.padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
        look = NoticeLook.CENTERED,
        action = ErrorAction(stringResource(R.string.retry), onClick = upcoming::retry),
    )
}

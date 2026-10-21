package fm.bae.app.ui.playback

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.displayError
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.QueueFailures")

/**
 * The queue sheet's failures as the person sees them: the last queue command that failed, shown at
 * the top of the sheet, since the library's banner sits under it.
 */
internal class QueueFailures(
    private val context: Context,
) {
    var command by mutableStateOf<DisplayError?>(null)
        private set

    /** Run a queue command; a failure is logged and shown, a success clears the last one. */
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
                logger.error("$operation failed", e)
                DisplayError(e.toString())
            }
    }
}

@Composable
internal fun rememberQueueFailures(): QueueFailures {
    val context = LocalContext.current
    return remember { QueueFailures(context) }
}

/** The last queue command that failed, above the lanes; nothing while none has. */
@Composable
internal fun QueueCommandFailure(failures: QueueFailures) {
    val failure = failures.command ?: return
    ErrorDetailDisclosure(
        error = failure,
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    )
}

/**
 * Why the read of the context's tail stopped, under its lane, with Retry, which opens it again;
 * nothing while it reads. Rows past the first window stay placeholders until it does.
 */
@Composable
internal fun UpcomingFailure(session: OpenLibrary) {
    val upcoming = session.playback.upcoming
    val failure by upcoming.failure.collectAsState()
    val shown = failure?.let { LocalContext.current.displayError(it) } ?: return
    Column(modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related)) {
        ErrorDetailDisclosure(shown)
        TextButton(onClick = upcoming::retry) { Text(stringResource(R.string.retry)) }
    }
}

package fm.bae.app.ui.components

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError

/**
 * What a notice's Details button opens: a [title], an optional [message], the
 * [causes] one a line, and [copyText], the whole diagnostic to copy.
 */
data class ErrorDetails(
    val title: String,
    val message: String?,
    val causes: String,
    val copyText: String,
) {
    companion object {
        /**
         * A notice showing the error's own line: that line is the title and the diagnostic the
         * causes; none without a diagnostic.
         */
        fun of(error: DisplayError): ErrorDetails? {
            val detail = error.detail ?: return null
            return ErrorDetails(error.line, null, error.detailExcerpt.orEmpty(), detail)
        }

        /**
         * A notice titled in the app's own words: the error's line leads the causes and the copy,
         * and the advice is the message.
         */
        fun titled(
            title: String,
            advice: String?,
            error: DisplayError,
        ): ErrorDetails {
            val detail = error.detail
            return ErrorDetails(
                title = title,
                message = advice,
                causes = if (detail == null) error.line else error.line + "\n" + error.detailExcerpt.orEmpty(),
                copyText = if (detail == null) error.line else error.line + "\n" + detail,
            )
        }
    }
}

/** [details] in a dialog: the title, the message, the causes panel with its copy button, and the system's OK. */
@Composable
fun ErrorDetailsDialog(
    details: ErrorDetails,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = details.title,
        message = details.message,
        content = { AndroidErrorDetails(text = details.causes, copyText = details.copyText) },
        confirmButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                onClick = onDismiss,
            ) { Text(stringResource(android.R.string.ok)) }
        },
    )
}

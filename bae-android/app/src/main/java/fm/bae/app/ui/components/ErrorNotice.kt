package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A notice's primary button: what to do about the failure. On its own line
 * under the words, in line with them; at the right beside Details when the
 * notice has both.
 */
class ErrorAction(
    val label: String,
    val enabled: Boolean = true,
    val onClick: () -> Unit,
)

/**
 * A failure as Android's error notice draws it: a [tone] notice (danger unless
 * told otherwise) with the tone's glyph and the localized line. A notice never
 * shows the error itself: for a failure core reported, a Details button opens
 * the causes in a dialog. [action] sits beside Details at the right, or alone
 * on its own line. Mirrors the Apple apps' `ErrorNotice`.
 */
@Composable
fun ErrorNotice(
    error: DisplayError,
    modifier: Modifier = Modifier,
    action: ErrorAction? = null,
    tone: StatusTone = StatusTone.DANGER,
) {
    ErrorNoticeContent(error.line, null, ErrorDetails.of(error), modifier, action, tone)
}

/**
 * A failure under a title the app words for what failed, with [advice], what
 * to do about it. [error] is null when core says the failure has nothing to
 * show; with one, Details opens its causes led by the category's line.
 */
@Composable
fun TitledErrorNotice(
    title: String,
    advice: String?,
    error: DisplayError?,
    modifier: Modifier = Modifier,
    action: ErrorAction? = null,
    tone: StatusTone = StatusTone.DANGER,
) {
    ErrorNoticeContent(title, advice, error?.let { ErrorDetails.titled(title, advice, it) }, modifier, action, tone)
}

/** On a tint, secondary text is primary text made see-through. */
private val onTintSecondary: Color
    @Composable
    get() = MaterialTheme.colorScheme.onSurface.copy(alpha = ON_TINT_SECONDARY_ALPHA)

private const val ON_TINT_SECONDARY_ALPHA = 0.72f

@Composable
private fun ErrorNoticeContent(
    title: String,
    advice: String?,
    details: ErrorDetails?,
    modifier: Modifier,
    action: ErrorAction?,
    tone: StatusTone,
) {
    var detailsShown by rememberSaveable(details) { mutableStateOf(false) }
    Notice(tone = tone, modifier = modifier) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
            Text(
                text = title,
                style = (if (advice != null) ThemeText.strong else ThemeText.body).style,
                color = MaterialTheme.colorScheme.onSurface,
            )
            advice?.let { Text(text = it, style = ThemeText.body.style, color = onTintSecondary) }
            if (details != null) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.related),
                    horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End),
                ) {
                    TextButton(onClick = { detailsShown = true }, contentPadding = ACTION_PADDING) {
                        Text(stringResource(R.string.details))
                    }
                    action?.let { ErrorActionButton(it) }
                }
            } else {
                action?.let { ErrorActionButton(it, Modifier.offset(x = -ThemeSpace.related)) }
            }
        }
    }
    if (detailsShown && details != null) {
        ErrorDetailsDialog(details, onDismiss = { detailsShown = false })
    }
}

private val ACTION_PADDING = PaddingValues(horizontal = ThemeSpace.related)

@Composable
private fun ErrorActionButton(
    action: ErrorAction,
    modifier: Modifier = Modifier,
) {
    TextButton(
        onClick = action.onClick,
        enabled = action.enabled,
        contentPadding = ACTION_PADDING,
        modifier = modifier,
    ) { Text(action.label) }
}

/** The causes, one a line, in an inset panel with a copy button at its corner. */
@Composable
fun CausesPanel(
    text: String,
    copyText: String,
    modifier: Modifier = Modifier,
) {
    val copy = rememberCopyText()
    Box(
        modifier =
            modifier
                .fillMaxWidth()
                .background(Color.Black.copy(alpha = CAUSES_PANEL_SHADE), RoundedCornerShape(ThemeRadius.control)),
    ) {
        SelectionContainer(
            modifier =
                Modifier.padding(
                    start = ThemeSpace.related,
                    top = ThemeSpace.related,
                    bottom = ThemeSpace.related,
                    end = CAUSES_COPY_INSET,
                ),
        ) {
            Text(text = text, style = ThemeText.mono.style, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onClick = { copy(copyText) }, modifier = Modifier.align(Alignment.TopEnd)) {
            Icon(
                imageVector = Icons.Filled.ContentCopy,
                contentDescription = stringResource(R.string.copy_details),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(ThemeIcon.medium),
            )
        }
    }
}

/** Room at the panel's right for the copy button. */
private val CAUSES_COPY_INSET = ThemeSpace.page + ThemeSpace.group

private const val CAUSES_PANEL_SHADE = 0.28f

@Preview(showBackground = true)
@Composable
private fun ErrorNoticePreview() {
    BaeTheme {
        ErrorNotice(
            DisplayError(
                "Couldn't reach the network.",
                "MusicBrainz network error: error sending request for url (https://musicbrainz.org/…): " +
                    "client error (Connect): dns error: failed to lookup address information",
            ),
            action = ErrorAction("Retry") {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun TitledErrorNoticePreview() {
    BaeTheme {
        TitledErrorNotice(
            title = "Couldn't read the sign-in settings",
            advice = "Dropbox and OneDrive sign-in are unavailable until this build is reinstalled.",
            error = DisplayError("OAuthConfig: missing client id"),
        )
    }
}

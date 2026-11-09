package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
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
    look: NoticeLook = NoticeLook.FILLED,
    onDismiss: (() -> Unit)? = null,
) {
    ErrorNoticeContent(error.line, null, ErrorDetails.of(error), modifier, action, tone, look, onDismiss)
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
    look: NoticeLook = NoticeLook.FILLED,
) {
    ErrorNoticeContent(
        title,
        advice,
        error?.let { ErrorDetails.titled(title, advice, it) },
        modifier,
        action,
        tone,
        look,
        null,
    )
}

/** On a notice's fill, secondary text is its text made see-through. */
private val onNoticeSecondary: Color
    @Composable
    get() = LocalContentColor.current.copy(alpha = ON_NOTICE_SECONDARY_ALPHA)

private const val ON_NOTICE_SECONDARY_ALPHA = 0.72f

@Composable
private fun ErrorNoticeContent(
    title: String,
    advice: String?,
    details: ErrorDetails?,
    modifier: Modifier,
    action: ErrorAction?,
    tone: StatusTone,
    look: NoticeLook,
    onDismiss: (() -> Unit)?,
) {
    var detailsShown by rememberSaveable(details) { mutableStateOf(false) }
    if (look == NoticeLook.CENTERED) {
        CenteredNotice(title, tone, modifier, details != null, { detailsShown = true }, action)
    } else {
        FramedNotice(title, advice, details != null, { detailsShown = true }, modifier, action, tone, look, onDismiss)
    }
    if (detailsShown && details != null) {
        ErrorDetailsDialog(details, onDismiss = { detailsShown = false })
    }
}

/**
 * A view that failed whole: in the middle of the space it's given, the title, then Details and
 * the action in one row.
 */
@Composable
private fun CenteredNotice(
    title: String,
    tone: StatusTone,
    modifier: Modifier,
    hasDetails: Boolean,
    onDetails: () -> Unit,
    action: ErrorAction?,
) {
    Column(
        modifier = modifier.fillMaxSize().heightIn(min = CENTERED_MIN_HEIGHT).padding(ThemeSpace.section),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group, Alignment.CenterVertically),
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.compact),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            StatusGlyph(tone, outline = true, muted = true)
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (hasDetails) {
                TextButton(onClick = onDetails) { Text(stringResource(R.string.details)) }
            }
            action?.let { OutlinedButton(onClick = it.onClick, enabled = it.enabled) { Text(it.label) } }
        }
    }
}

/** What a centred notice stands at least this tall, where its space is unbounded. */
private val CENTERED_MIN_HEIGHT = 300.dp

@Composable
private fun FramedNotice(
    title: String,
    advice: String?,
    hasDetails: Boolean,
    onDetails: () -> Unit,
    modifier: Modifier,
    action: ErrorAction?,
    tone: StatusTone,
    look: NoticeLook,
    onDismiss: (() -> Unit)?,
) {
    Notice(tone = tone, modifier = modifier, look = look) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
            Row(verticalAlignment = Alignment.Top) {
                Text(
                    text = title,
                    style = (if (advice != null) ThemeText.strong else ThemeText.body).style,
                    color = LocalContentColor.current,
                    modifier = Modifier.weight(1f),
                )
                onDismiss?.let { DismissButton(it) }
            }
            advice?.let { Text(text = it, style = ThemeText.body.style, color = onNoticeSecondary) }
            if (hasDetails) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(top = ThemeSpace.related),
                    horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related, Alignment.End),
                ) {
                    TextButton(
                        onClick = onDetails,
                        colors = ButtonDefaults.textButtonColors(contentColor = noticeButtonColor),
                        contentPadding = ACTION_PADDING,
                    ) {
                        Text(stringResource(R.string.details))
                    }
                    action?.let { ErrorActionButton(it) }
                }
            } else {
                action?.let { ErrorActionButton(it, Modifier.offset(x = -ThemeSpace.related)) }
            }
        }
    }
}

/** The × at a notice's right: a 20dp glyph whose touch area overhangs the notice's padding. */
@Composable
private fun DismissButton(onDismiss: () -> Unit) {
    IconButton(onClick = onDismiss, modifier = Modifier.size(ThemeIcon.medium).wrapContentSize(unbounded = true)) {
        Icon(
            imageVector = Icons.Filled.Close,
            contentDescription = stringResource(R.string.dismiss),
            tint = onNoticeSecondary,
            modifier = Modifier.size(ThemeIcon.medium),
        )
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
        colors = ButtonDefaults.textButtonColors(contentColor = noticeButtonColor),
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
                .background(BaeTheme.colors.insetPanel, RoundedCornerShape(ThemeRadius.control)),
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

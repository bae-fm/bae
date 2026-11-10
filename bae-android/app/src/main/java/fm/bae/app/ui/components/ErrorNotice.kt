package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
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
 * A notice's primary button: what to do about the failure. At the right of the
 * title's line, Details first when the notice has both.
 */
class ErrorAction(
    val label: String,
    val enabled: Boolean = true,
    val onClick: () -> Unit,
)

/** Where a notice's buttons sit in the framed layout. */
enum class NoticeButtons {
    /** On the title's line at the right, centred on it, the words wrapping beside them. */
    INLINE,

    /** On a line of their own under the words, in line with them: for a long button in a narrow place. */
    BELOW,
}

/**
 * A failure as Android's error notice draws it: a [tone] notice (danger unless
 * told otherwise) with the tone's glyph and the localized line. A notice never
 * shows the error itself: for a failure core reported, a Details button opens
 * the causes in a dialog. Details and [action] sit at the right of the title's
 * line, Details first, or under the words when [buttons] says so. Given
 * [onOpenDetails], the notice has no Details button: tapping it hands the
 * details to the caller, who shows them. Mirrors the Apple apps' `Notice`.
 */
@Composable
fun ErrorNotice(
    error: DisplayError,
    modifier: Modifier = Modifier,
    action: ErrorAction? = null,
    tone: StatusTone = StatusTone.DANGER,
    look: NoticeLook = NoticeLook.FILLED,
    onDismiss: (() -> Unit)? = null,
    onOpenDetails: ((ErrorDetails) -> Unit)? = null,
    buttons: NoticeButtons = NoticeButtons.INLINE,
) {
    ErrorNoticeContent(
        title = error.line,
        advice = null,
        details = ErrorDetails.of(error),
        modifier = modifier,
        action = action,
        tone = tone,
        look = look,
        onDismiss = onDismiss,
        onOpenDetails = onOpenDetails,
        buttons = buttons,
    )
}

/**
 * A failure under a title the app words for what failed, with [advice], what
 * to do about it. [error] is null when core says the failure has nothing to
 * show; with one, Details opens its causes led by the category's line, or a
 * tap hands them to [onOpenDetails], as [ErrorNotice] says.
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
    onDismiss: (() -> Unit)? = null,
    onOpenDetails: ((ErrorDetails) -> Unit)? = null,
    buttons: NoticeButtons = NoticeButtons.INLINE,
) {
    ErrorNoticeContent(
        title = title,
        advice = advice,
        details = error?.let { ErrorDetails.titled(title, advice, it) },
        modifier = modifier,
        action = action,
        tone = tone,
        look = look,
        onDismiss = onDismiss,
        onOpenDetails = onOpenDetails,
        buttons = buttons,
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
    onOpenDetails: ((ErrorDetails) -> Unit)?,
    buttons: NoticeButtons,
) {
    var detailsShown by rememberSaveable(details) { mutableStateOf(false) }
    if (look == NoticeLook.CENTERED) {
        CenteredNotice(title, tone, modifier, details != null, { detailsShown = true }, action)
    } else {
        FramedNotice(
            title = title,
            advice = advice,
            hasDetails = details != null && onOpenDetails == null,
            onDetails = { detailsShown = true },
            modifier =
                if (details != null && onOpenDetails != null) {
                    modifier.clickable { onOpenDetails(details) }
                } else {
                    modifier
                },
            action = action,
            tone = tone,
            look = look,
            onDismiss = onDismiss,
            buttons = buttons,
        )
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
                color = MaterialTheme.colorScheme.onSurface,
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
            action?.let { FilledTonalButton(onClick = it.onClick, enabled = it.enabled) { Text(it.label) } }
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
    buttons: NoticeButtons,
) {
    val hasButtons = hasDetails || action != null
    Notice(tone = tone, modifier = modifier, look = look) {
        Row(
            modifier = Modifier.weight(1f),
            verticalAlignment = Alignment.Top,
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
                Text(
                    text = title,
                    style = (if (advice != null) ThemeText.strong else ThemeText.body).style,
                    color = LocalContentColor.current,
                )
                advice?.let { Text(text = it, style = ThemeText.body.style, color = onNoticeSecondary) }
                if (hasButtons && buttons == NoticeButtons.BELOW) {
                    // The buttons' own padding overhangs: a label starts on the words' edge.
                    NoticeButtonRow(hasDetails, onDetails, action, Modifier.offset(x = -ThemeSpace.related))
                }
            }
            if (hasButtons && buttons == NoticeButtons.INLINE) {
                // The buttons' own padding overhangs: a label's right edge is the notice's.
                FirstLineBox {
                    NoticeButtonRow(hasDetails, onDetails, action, Modifier.offset(x = ThemeSpace.related))
                }
            }
            onDismiss?.let { DismissButton(it) }
        }
    }
}

/** A notice's buttons in a row: Details first, then the primary [action]. */
@Composable
private fun NoticeButtonRow(
    hasDetails: Boolean,
    onDetails: () -> Unit,
    action: ErrorAction?,
    modifier: Modifier,
) {
    Row(modifier = modifier, horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        if (hasDetails) {
            TextButton(
                onClick = onDetails,
                colors = ButtonDefaults.textButtonColors(contentColor = noticeButtonColor),
                contentPadding = ACTION_PADDING,
            ) {
                Text(stringResource(R.string.details))
            }
        }
        action?.let { ErrorActionButton(it) }
    }
}

/** The × at a notice's right. */
@Composable
private fun DismissButton(onDismiss: () -> Unit) {
    InlineIconButton(Icons.Filled.Close, stringResource(R.string.dismiss), onDismiss, tint = onNoticeSecondary)
}

private val ACTION_PADDING = PaddingValues(horizontal = ThemeSpace.related)

@Composable
private fun ErrorActionButton(action: ErrorAction) {
    TextButton(
        onClick = action.onClick,
        enabled = action.enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = noticeButtonColor),
        contentPadding = ACTION_PADDING,
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

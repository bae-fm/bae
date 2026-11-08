package fm.bae.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
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

/** A text button at the right of an error box, level with its message: what to do about the failure. */
class ErrorAction(
    val label: String,
    val enabled: Boolean = true,
    val onClick: () -> Unit,
)

/**
 * A failure as Android's error box draws it: a [tone] notice (danger unless
 * told otherwise) with the tone's glyph, the localized line in primary text,
 * then, for a failure core reported, a "Details" line with a chevron after the
 * word. Open, the causes show in an inset panel with a copy button at its
 * corner that copies the whole diagnostic. [action] sits at the box's right,
 * level with the line. [bare] draws the content without the box, for a
 * disclosure already inside a notice. Mirrors the Apple apps'
 * `ErrorDetailDisclosure`.
 */
@Composable
fun ErrorDetailDisclosure(
    error: DisplayError,
    modifier: Modifier = Modifier,
    action: ErrorAction? = null,
    tone: StatusTone = StatusTone.DANGER,
    bare: Boolean = false,
) {
    if (bare) {
        ErrorDetailContent(error, action, modifier)
    } else {
        Notice(tone = tone, modifier = modifier) {
            ErrorDetailContent(error, action, Modifier.weight(1f))
        }
    }
}

/** On a tint, secondary text is primary text made see-through. */
private val onTintSecondary: Color
    @Composable
    get() = MaterialTheme.colorScheme.onSurface.copy(alpha = ON_TINT_SECONDARY_ALPHA)

private const val ON_TINT_SECONDARY_ALPHA = 0.72f

@Composable
private fun ErrorDetailContent(
    error: DisplayError,
    action: ErrorAction?,
    modifier: Modifier,
) {
    var expanded by rememberSaveable(error) { mutableStateOf(false) }
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.line)) {
        Row(verticalAlignment = Alignment.Top) {
            Text(
                text = error.line,
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            action?.let { ErrorActionButton(it) }
        }
        val detail = error.detail ?: return@Column
        Row(
            modifier = Modifier.clickable { expanded = !expanded },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.line),
        ) {
            Text(text = stringResource(R.string.details), style = ThemeText.detail.style, color = onTintSecondary)
            Icon(
                imageVector = if (expanded) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                contentDescription = null,
                tint = onTintSecondary,
                modifier = Modifier.size(ThemeIcon.small),
            )
        }
        if (expanded) {
            CausesPanel(text = error.detailExcerpt.orEmpty(), copyText = detail)
        }
    }
}

@Composable
private fun ErrorActionButton(action: ErrorAction) {
    TextButton(
        onClick = action.onClick,
        enabled = action.enabled,
        contentPadding = PaddingValues(horizontal = ThemeSpace.related),
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
            Text(text = text, style = ThemeText.mono.style, color = onTintSecondary)
        }
        IconButton(onClick = { copy(copyText) }, modifier = Modifier.align(Alignment.TopEnd)) {
            Icon(
                imageVector = Icons.Filled.ContentCopy,
                contentDescription = stringResource(R.string.copy_details),
                tint = onTintSecondary,
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
private fun ErrorDetailDisclosurePreview() {
    BaeTheme {
        ErrorDetailDisclosure(
            DisplayError(
                "Couldn't reach the network.",
                "MusicBrainz network error: error sending request for url (https://musicbrainz.org/…): " +
                    "client error (Connect): dns error: failed to lookup address information",
            ),
            action = ErrorAction("Retry") {},
        )
    }
}

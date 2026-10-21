package fm.bae.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * A failure as a person sees it: the localized line; under it, for a failure
 * core reported, the concrete fault in one line with a copy action that copies
 * the whole diagnostic; and when the diagnostic says more than that line, a
 * Details disclosure showing its opening. Mirrors the Apple apps'
 * `ErrorDetailDisclosure`.
 */
@Composable
fun ErrorDetailDisclosure(
    error: DisplayError,
    modifier: Modifier = Modifier,
    lineColor: Color = BaeTheme.colors.danger,
) {
    var expanded by rememberSaveable(error) { mutableStateOf(false) }
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Text(text = error.line, style = ThemeText.body.style, color = lineColor)
        val detail = error.detail ?: return@Column
        val secondary = MaterialTheme.colorScheme.onSurfaceVariant
        Row(verticalAlignment = Alignment.Top) {
            SelectionContainer(modifier = Modifier.weight(1f)) {
                Text(
                    text = error.detailSummary.orEmpty(),
                    style = ThemeText.mono.style,
                    color = secondary,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            val clipboard = LocalClipboardManager.current
            IconButton(onClick = { clipboard.setText(AnnotatedString(detail)) }) {
                Icon(
                    imageVector = Icons.Filled.ContentCopy,
                    contentDescription = stringResource(R.string.copy_details),
                    tint = secondary,
                    modifier = Modifier.size(ThemeIcon.medium),
                )
            }
        }
        // Only when the diagnostic says more than the line above.
        val excerpt = error.detailExcerpt
        if (excerpt != null && detail != error.detailSummary) {
            Row(
                modifier = Modifier.fillMaxWidth().clickable { expanded = !expanded },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
            ) {
                Icon(
                    imageVector =
                        if (expanded) Icons.Filled.KeyboardArrowDown else Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = null,
                    tint = secondary,
                )
                Text(text = stringResource(R.string.details), style = ThemeText.detail.style, color = secondary)
            }
            if (expanded) {
                SelectionContainer {
                    Text(
                        text = excerpt,
                        style = ThemeText.mono.style,
                        color = secondary,
                        maxLines = 6,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

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
        )
    }
}

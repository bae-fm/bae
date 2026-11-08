package fm.bae.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * A view that failed whole, with no box: the message centred in secondary
 * text; under it one row, "Show details" (when the failure has a diagnostic)
 * then Retry as an outlined button; open, the link reads "Hide details" and
 * the causes panel sits under that row.
 */
@Composable
fun LoadFailure(
    error: DisplayError,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var expanded by rememberSaveable(error) { mutableStateOf(false) }
    Column(
        modifier = modifier.padding(ThemeSpace.section),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Text(
            text = error.line,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        Row(
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (error.detail != null) {
                Text(
                    text = stringResource(if (expanded) R.string.hide_details else R.string.show_details),
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.clickable { expanded = !expanded },
                )
            }
            OutlinedButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
        }
        val detail = error.detail
        if (detail != null && expanded) {
            CausesPanel(text = error.detailExcerpt.orEmpty(), copyText = detail)
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun LoadFailurePreview() {
    BaeTheme {
        LoadFailure(
            DisplayError("Couldn't load your library", "database error: disk I/O error"),
            onRetry = {},
        )
    }
}

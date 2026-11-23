package fm.bae.app.ui.components

import androidx.compose.runtime.Composable

/**
 * A card whose first row is the [label] (with a [summary] under it), `expand_more` at its end when
 * closed and `expand_less` when [open]; tapping it calls [onToggle]. When open, [content] (rows of the
 * same card) follows it.
 */
@Composable
internal fun AndroidDisclosure(
    label: String,
    open: Boolean,
    onToggle: () -> Unit,
    summary: String? = null,
    content: @Composable () -> Unit,
) {
    AndroidCard {
        AndroidRow(
            title = label,
            subtitle = summary,
            onClick = onToggle,
            icon = if (open) RowIcon.EXPAND_LESS else RowIcon.EXPAND_MORE,
        )
        if (open) content()
    }
}

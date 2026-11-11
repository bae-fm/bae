package fm.bae.app.ui.components

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
    content: @Composable ColumnScope.() -> Unit,
) {
    AndroidCard {
        AndroidRow(
            title = label,
            subtitle = summary,
            onClick = onToggle,
            trailing = {
                Icon(
                    imageVector = if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            },
        )
        if (open) content()
    }
}

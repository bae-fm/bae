package fm.bae.app.ui.library

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidMenuEntry

/**
 * One of a menu's choices, the chosen one in the accent color. A [check] ends the row with the accent check that
 * marks the person's own choice, and [content] draws the choice in place of its [label] words (the rating's
 * stars), with the label as what a screen reader reads.
 */
@Composable
internal fun AndroidMenuItem(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
    enabled: Boolean = true,
    check: Boolean = false,
    content: (@Composable () -> Unit)? = null,
) {
    // TalkBack reads Selected as the chosen item's state, because the design draws no selected state for a menu item.
    val chosen = stringResource(R.string.menu_item_chosen)
    AndroidMenuEntry(
        text = {
            if (content != null) {
                content()
            } else {
                Text(
                    text = label,
                    color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                    style = if (selected) ThemeText.strong.style else ThemeText.androidLabel.style,
                )
            }
        },
        onClick = onClick,
        enabled = enabled,
        trailingIcon =
            if (check) {
                {
                    Icon(
                        imageVector = SymbolIcons.Check,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(ThemeIcon.medium),
                    )
                }
            } else {
                null
            },
        modifier =
            Modifier.semantics(mergeDescendants = true) {
                if (selected || check) stateDescription = chosen
                if (content != null) contentDescription = label
            },
    )
}

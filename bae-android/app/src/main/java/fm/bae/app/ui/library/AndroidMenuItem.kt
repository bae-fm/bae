package fm.bae.app.ui.library

import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import fm.bae.app.R

/** One of a menu's choices, the chosen one in the accent color. */
@Composable
internal fun AndroidMenuItem(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    // TalkBack reads Selected as the chosen item's state, because the design draws no selected state for a menu item.
    val chosen = stringResource(R.string.menu_item_chosen)
    DropdownMenuItem(
        text = {
            Text(
                text = label,
                color = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
                fontWeight = if (selected) FontWeight.SemiBold else null,
            )
        },
        onClick = onClick,
        modifier = Modifier.semantics { if (selected) stateDescription = chosen },
    )
}

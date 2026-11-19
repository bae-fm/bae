package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.R

/**
 * The ⋮ button of a row, with the row's actions in its menu: a row offers its actions here, not as a button of each.
 * [items] gets a function that closes the menu, for an item to call before it acts.
 */
@Composable
fun AndroidMoreMenu(
    enabled: Boolean = true,
    items: @Composable ColumnScope.(close: () -> Unit) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(Icons.Filled.MoreVert, stringResource(R.string.more), onClick = { expanded = true }, enabled = enabled)
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            items { expanded = false }
        }
    }
}

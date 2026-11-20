package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.offset
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R

/**
 * The ⋮ button of a row, with the row's actions in its menu: a row offers its actions here, not as a button of each.
 * [items] gets a function that closes the menu, for an item to call before it acts. With [drawnOpen] the menu is
 * drawn open under the button in place, for a shot, which holds no popup.
 */
@Composable
fun AndroidMoreMenu(
    enabled: Boolean = true,
    drawnOpen: Boolean = false,
    items: @Composable ColumnScope.(close: () -> Unit) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(
            Icons.Filled.MoreVert,
            stringResource(R.string.more),
            onClick = { expanded = true },
            enabled = enabled,
        )
        if (drawnOpen) {
            AndroidMenuInPlace(modifier = Modifier.align(Alignment.TopEnd).offset(y = DRAWN_MENU_TOP)) {
                items { }
            }
        } else {
            AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                items { expanded = false }
            }
        }
    }
}

private val DRAWN_MENU_TOP = 40.dp

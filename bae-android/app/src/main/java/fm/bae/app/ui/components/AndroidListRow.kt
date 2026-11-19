package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import uniffi.bae_bridge.BridgeImageRef

/**
 * A playlist's track row: the track's cover, title and [subtitle], its [trailing] length, and the row's
 * [menu] (its ⋮) at the end when it has one. A tap on the row runs [onClick].
 */
@Composable
internal fun AndroidListRow(
    title: String,
    subtitle: String,
    trailing: String,
    art: BridgeImageRef?,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    menu: (@Composable () -> Unit)? = null,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier.fillMaxWidth().padding(end = if (menu != null) MENU_END_PADDING else 0.dp),
    ) {
        Box(modifier = Modifier.weight(1f)) {
            AndroidLibraryRow(title = title, subtitle = subtitle, trailing = trailing, art = art, onClick = onClick)
        }
        menu?.invoke()
    }
}

private val MENU_END_PADDING = 4.dp

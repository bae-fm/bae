package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.R

/**
 * A track's options button and its menu: Play next, Add to queue, then Add to playlist… when the caller
 * passes [onAddToPlaylist] (the person can make playlists), and Remove from playlist when the caller
 * passes [onRemove] (a track in a playlist of the person's own).
 */
@Composable
internal fun TrackOptionsMenu(
    onPlayNext: () -> Unit,
    onAddToQueue: () -> Unit,
    onAddToPlaylist: (() -> Unit)? = null,
    onRemove: (() -> Unit)? = null,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(
            icon = Icons.Filled.MoreVert,
            contentDescription = stringResource(R.string.track_options),
            onClick = { expanded = true },
            tone = IconTone.MUTED,
        )
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            TrackOption(R.string.play_next, onPlayNext) { expanded = false }
            TrackOption(R.string.add_to_queue, onAddToQueue) { expanded = false }
            onAddToPlaylist?.let { TrackOption(R.string.playlist_add_entry, it) { expanded = false } }
            onRemove?.let { TrackOption(R.string.playlist_remove_entry, it) { expanded = false } }
        }
    }
}

@Composable
private fun TrackOption(
    label: Int,
    onClick: () -> Unit,
    close: () -> Unit,
) {
    DropdownMenuItem(
        text = { Text(stringResource(label)) },
        onClick = {
            close()
            onClick()
        },
    )
}

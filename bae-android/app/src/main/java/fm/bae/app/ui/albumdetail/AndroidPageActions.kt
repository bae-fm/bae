package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.ButtonKind

/**
 * An album's or playlist's buttons: Play and Shuffle, and under them Play next and Add to queue when the
 * page offers them.
 */
@Composable
internal fun AndroidPageActions(
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    modifier: Modifier = Modifier,
    onPlayNext: (() -> Unit)? = null,
    onAddToQueue: (() -> Unit)? = null,
) {
    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
            AndroidButton(onClick = onPlay) {
                Text(stringResource(R.string.play))
            }
            AndroidButton(kind = ButtonKind.OUTLINED, onClick = onShuffle) {
                Text(stringResource(R.string.shuffle))
            }
        }
        if (onPlayNext != null || onAddToQueue != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                onPlayNext?.let {
                    AndroidButton(kind = ButtonKind.OUTLINED, onClick = it) {
                        Text(stringResource(R.string.play_next))
                    }
                }
                onAddToQueue?.let {
                    AndroidButton(kind = ButtonKind.OUTLINED, onClick = it) {
                        Text(stringResource(R.string.add_to_queue))
                    }
                }
            }
        }
    }
}

package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace

/** Play next and Add to queue for a whole page: an album's offers both, a playlist's neither. */
internal class QueueActions(
    val onPlayNext: () -> Unit,
    val onAddToQueue: () -> Unit,
)

/** An album's or playlist's buttons: Play and Shuffle, and under them the page's [queue] actions when it has them. */
@Composable
internal fun AndroidPageActions(
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    modifier: Modifier = Modifier,
    queue: QueueActions? = null,
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
        queue?.let {
            Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                AndroidButton(kind = ButtonKind.OUTLINED, onClick = it.onPlayNext) {
                    Text(stringResource(R.string.play_next))
                }
                AndroidButton(kind = ButtonKind.OUTLINED, onClick = it.onAddToQueue) {
                    Text(stringResource(R.string.add_to_queue))
                }
            }
        }
    }
}

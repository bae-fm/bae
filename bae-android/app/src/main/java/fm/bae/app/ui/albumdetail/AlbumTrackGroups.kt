package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import fm.bae.app.creditLineText
import fm.bae.app.durationClockLabel
import fm.bae.app.durationUnitsText
import fm.bae.app.sideHeaderText
import fm.bae.app.text
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import uniffi.bae_bridge.BridgeRelease

/**
 * The release's tracks by side: each side's header, its rows touching, and its play time when there are
 * several sides.
 */
internal fun LazyListScope.albumTrackGroups(
    release: BridgeRelease,
    positionColumnWidth: Dp,
    playback: AlbumPlaybackState,
    callbacks: AlbumDetailCallbacks,
    context: android.content.Context,
) {
    release.trackGroups.forEach { group ->
        val header = group.sideHeaderText(context)
        if (header.isNotEmpty()) {
            item {
                AndroidEyebrow(text = header)
            }
        }
        items(group.tracks, key = { it.id }) { track ->
            val isCurrent = track.id == playback.currentTrackId
            AndroidTrackRow(
                data =
                    TrackRowData(
                        positionLabel = track.positionText,
                        title = track.title,
                        artistNames = track.displayArtist?.let(context::creditLineText),
                        durationLabel = context.durationClockLabel(track.durationClock),
                        isCurrent = isCurrent,
                        isPlaying = playback.isPlaying,
                    ),
                callbacks =
                    TrackRowCallbacks(
                        onClick = {
                            if (isCurrent) {
                                callbacks.onTogglePlayPause()
                            } else {
                                callbacks.onPlayTrack(track.id)
                            }
                        },
                        onPlayNext = { callbacks.onPlayTrackNext(track.id) },
                        onAddToQueue = { callbacks.onAddTrackToQueue(track.id) },
                    ),
                positionColumnWidth = positionColumnWidth,
            )
        }
        // The header shows the total, so only a release core says shows its
        // sides' time lists each side's play time.
        val groupDurationLabel = context.durationUnitsText(group.totalDuration)
        if (group.showsTime && groupDurationLabel.isNotEmpty()) {
            item {
                Text(
                    text = groupDurationLabel,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = ThemeSpace.related),
                )
            }
        }
    }
}

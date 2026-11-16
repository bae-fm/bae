package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import fm.bae.app.coreString
import fm.bae.app.creditLineText
import fm.bae.app.durationClockLabel
import fm.bae.app.durationUnitsText
import fm.bae.app.sideHeaderText
import fm.bae.app.text
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidEyebrow
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeTrackWork

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
            Column {
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
                            onAddToPlaylist = callbacks.onAddTrackToPlaylist?.let { add -> { add(track.id) } },
                        ),
                    positionColumnWidth = positionColumnWidth,
                )
                track.works.forEach { work ->
                    TrackWorkLine(work, positionColumnWidth, callbacks.onSelectWork, context)
                }
            }
        }
        // The header shows the total, so the list shows a side's play time
        // only when core sets showsTime on its group.
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

/**
 * Shows a work the track performs under the track's row, lined up with the title: the work's title in the
 * accent colour, then its composers. A tap opens the work.
 */
@Composable
private fun TrackWorkLine(
    work: BridgeTrackWork,
    positionColumnWidth: Dp,
    onSelectWork: (String) -> Unit,
    context: android.content.Context,
) {
    val text =
        listOfNotNull(work.title, work.composerNames)
            .joinToString(context.coreString("core.audio.list_separator"))
    Text(
        text = text,
        style = ThemeText.fine.style,
        color = MaterialTheme.colorScheme.primary,
        modifier =
            Modifier
                .clickable { onSelectWork(work.workId) }
                .padding(start = positionColumnWidth, bottom = ThemeSpace.compact),
    )
}

package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.SkipPrevious
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CoverImage
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.color
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeRepeatMode

/**
 * The bar itself: the track, or the failed read in its place, beside the
 * [transport], over the seek row ([progress]) and why a cast ended.
 */
@Composable
internal fun AndroidNowPlayingBar(
    track: NowPlaying?,
    failure: DisplayError?,
    onExpand: () -> Unit,
    transport: @Composable () -> Unit,
    progress: @Composable () -> Unit,
    castEnded: @Composable () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 3.dp) {
        Column(modifier = Modifier.fillMaxWidth()) {
            // A hairline along the top edge, so what scrolls above doesn't run
            // straight into the bar.
            HorizontalDivider(color = BaeTheme.colors.hairline)
            Column(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .padding(horizontal = ThemeSpace.group, vertical = ThemeSpace.related),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (failure != null) {
                        ErrorText(message = failure.line, maxLines = 2, modifier = Modifier.weight(1f))
                    } else if (track != null) {
                        NowPlayingTrackInfo(
                            track = track,
                            onExpand = onExpand,
                        )
                    }
                    transport()
                }
                progress()
                castEnded()
            }
        }
    }
}

@Composable
private fun RowScope.NowPlayingTrackInfo(
    track: fm.bae.app.playback.NowPlaying,
    onExpand: () -> Unit,
) {
    val nowPlayingDescription = stringResource(R.string.now_playing_track_by_artist, track.title, track.artist)
    Row(
        modifier =
            Modifier
                .weight(1f)
                .clickable(onClick = onExpand)
                // One TalkBack element named for the track, instead of an
                // unnamed button and loose text.
                .semantics(mergeDescendants = true) { contentDescription = nowPlayingDescription },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = track.coverImage,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.barArtwork),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = track.title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
                modifier = Modifier.weight(1f, fill = false),
            )
            Text(
                text = track.artist,
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

/** The bar's transport: previous, play/pause, next, cast, the queue and repeat. */
@Composable
internal fun NowPlayingTransport(
    isPlaying: Boolean,
    storageWait: LatestSource<Boolean>?,
    repeatMode: BridgeRepeatMode,
    onPrevious: () -> Unit,
    onPlayPause: () -> Unit,
    onNext: () -> Unit,
    castButton: @Composable () -> Unit,
    onOpenQueue: () -> Unit,
    onCycleRepeat: () -> Unit,
) {
    IconButton(onClick = onPrevious) {
        Icon(Icons.Filled.SkipPrevious, contentDescription = stringResource(R.string.player_previous))
    }
    AndroidPlayPause(
        isPlaying = isPlaying,
        iconSize = ThemeIcon.large,
        storageWait = storageWait,
        onToggle = onPlayPause,
    )
    IconButton(onClick = onNext) {
        Icon(Icons.Filled.SkipNext, contentDescription = stringResource(R.string.player_next))
    }
    castButton()
    IconButton(onClick = onOpenQueue) {
        Icon(Icons.AutoMirrored.Filled.List, contentDescription = stringResource(R.string.queue))
    }
    IconButton(onClick = onCycleRepeat) {
        Icon(
            imageVector = if (repeatMode == BridgeRepeatMode.TRACK) Icons.Filled.RepeatOne else Icons.Filled.Repeat,
            contentDescription = stringResource(R.string.player_repeat),
            tint =
                if (repeatMode == BridgeRepeatMode.OFF) {
                    MaterialTheme.colorScheme.onSurfaceVariant
                } else {
                    MaterialTheme.colorScheme.primary
                },
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun NowPlayingTrackInfoPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            Row {
                NowPlayingTrackInfo(
                    track =
                        NowPlaying(
                            trackId = "trk-1",
                            title = "Track Title",
                            artist = "Artist Name",
                            coverImage = PreviewData.imageRef("rel-1"),
                            sidePausePrompt = null,
                        ),
                    onExpand = {},
                )
            }
        }
    }
}

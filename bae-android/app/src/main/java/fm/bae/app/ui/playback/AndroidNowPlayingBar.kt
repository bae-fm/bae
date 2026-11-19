package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Repeat
import androidx.compose.material.icons.filled.RepeatOne
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.creditLineText
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.library
import fm.bae.app.playback.title
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidErrorLine
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.Artwork
import fm.bae.app.ui.components.ICON_BUTTON_BAR_BOX
import fm.bae.app.ui.components.IconTone
import fm.bae.app.ui.components.color
import fm.bae.app.ui.components.liftsSnackbar
import uniffi.bae_bridge.BridgeNowPlayingDisplay
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
    Surface(
        modifier = Modifier.liftsSnackbar(),
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 3.dp,
    ) {
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
                        AndroidErrorLine(message = failure.line, maxLines = 2, modifier = Modifier.weight(1f))
                    } else if (track != null) {
                        AndroidNowPlayingTrack(
                            track = track,
                            onExpand = onExpand,
                            modifier = Modifier.weight(1f),
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

/** The playing track: its cover, title and artists, which open the full player as one TalkBack element. */
@Composable
internal fun AndroidNowPlayingTrack(
    track: NowPlaying,
    onExpand: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val title = track.display.title(context)
    // The bar names a library track to TalkBack by its title and artists, and a track the library no longer holds,
    // which has no artist, by its title alone.
    val artist: String?
    val nowPlayingDescription: String
    when (val display = track.display) {
        is BridgeNowPlayingDisplay.Library -> {
            val credited = context.creditLineText(display.display.artistLine)
            artist = credited
            nowPlayingDescription = stringResource(R.string.now_playing_track_by_artist, title, credited)
        }

        BridgeNowPlayingDisplay.Removed -> {
            artist = null
            nowPlayingDescription = title
        }
    }
    Row(
        modifier =
            modifier
                .clickable(onClick = onExpand)
                // One TalkBack element named for the track, instead of an
                // unnamed button and loose text.
                .semantics(mergeDescendants = true) { contentDescription = nowPlayingDescription },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Artwork(
            cover = track.display.library?.coverImage,
            cornerRadius = ThemeRadius.artwork,
            iconPadding = ThemeSpace.group,
            modifier = Modifier.size(ThemeSize.barArtwork),
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = ThemeText.rowTitle.style,
                maxLines = 1,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (artist != null) {
                Text(
                    text = artist,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
    }
}

/** The bar's controls: the [AndroidTransport], cast, the queue and repeat. */
@Composable
internal fun NowPlayingBarControls(
    isPlaying: Boolean,
    waitsOnStorage: Boolean,
    repeatMode: BridgeRepeatMode,
    onPrevious: () -> Unit,
    onPlayPause: () -> Unit,
    onNext: () -> Unit,
    castButton: @Composable () -> Unit,
    onOpenQueue: () -> Unit,
    onCycleRepeat: () -> Unit,
) {
    AndroidTransport(
        isPlaying = isPlaying,
        waitsOnStorage = waitsOnStorage,
        onPrevious = onPrevious,
        onPlayPause = onPlayPause,
        onNext = onNext,
    )
    castButton()
    AndroidIconButton(
        Icons.AutoMirrored.Filled.List,
        stringResource(R.string.queue),
        onClick = onOpenQueue,
        box = ICON_BUTTON_BAR_BOX,
    )
    AndroidIconButton(
        icon = if (repeatMode == BridgeRepeatMode.TRACK) Icons.Filled.RepeatOne else Icons.Filled.Repeat,
        contentDescription = stringResource(R.string.player_repeat),
        onClick = onCycleRepeat,
        tone = if (repeatMode == BridgeRepeatMode.OFF) IconTone.MUTED else IconTone.ACCENT,
        box = ICON_BUTTON_BAR_BOX,
    )
}

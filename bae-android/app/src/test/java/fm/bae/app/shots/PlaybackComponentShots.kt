package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.AndroidNowPlayingBar
import fm.bae.app.ui.playback.AndroidPlayPause
import fm.bae.app.ui.playback.AndroidQueuePlaceholderRow
import fm.bae.app.ui.playback.AndroidQueueRow
import fm.bae.app.ui.playback.AndroidQueueSection
import fm.bae.app.ui.playback.AndroidTransport
import fm.bae.app.ui.playback.AndroidVolumeSlider
import fm.bae.app.ui.playback.CastBarButton
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.NowPlayingBarControls
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackSpan

internal val playing =
    PlayerStaging(progress = 0.35, elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)))
internal val waiting =
    PlayerStaging(
        progress = 0.02,
        elapsedSeconds = 5,
        held = listOf(BridgeTrackSpan(0.0, 0.04)),
        waitsOnStorage = true,
    )
private val holes =
    PlayerStaging(
        progress = 0.2,
        elapsedSeconds = 61,
        held = listOf(BridgeTrackSpan(0.0, 0.3), BridgeTrackSpan(0.55, 0.7)),
    )

/** The component shots of the player: the bar, its play and pause, the seek row, the full player and the queue's rows. */
class PlaybackComponentShots : BoardShot() {
    @Test
    fun androidComponentNowPlayingBarPlaying() = component("android.component.now-playing-bar.playing", wide = true) { Bar(playing) }

    @Test
    fun androidComponentNowPlayingBarWaitingOnTheCloud() =
        component("android.component.now-playing-bar.waiting-on-the-cloud", wide = true) { Bar(waiting) }

    @Test
    fun androidComponentPlayPausePaused() =
        component("android.component.play-pause.paused") { AndroidPlayPause(false, ThemeIcon.large, false) {} }

    @Test
    fun androidComponentPlayPausePlaying() =
        component("android.component.play-pause.playing") { AndroidPlayPause(true, ThemeIcon.large, false) {} }

    @Test
    fun androidComponentPlayPauseWaiting() =
        component("android.component.play-pause.waiting") { AndroidPlayPause(true, ThemeIcon.large, waiting.waitsOnStorage) {} }

    @Test
    fun androidComponentPlayPauseFullPlayer() =
        component("android.component.play-pause.full-player") { AndroidPlayPause(true, 48.dp, false) {} }

    @Test
    fun androidComponentSeekBarPlaying() = component("android.component.seek-bar.playing", wide = true) { playing.SeekRow() }

    @Test
    fun androidComponentSeekBarHoles() = component("android.component.seek-bar.holes", wide = true) { holes.SeekRow() }

    @Test
    fun androidComponentFullPlayerPlaying() = shot("android.component.full-player.playing") { FullPlayer(playing) }

    @Test
    fun androidComponentQueueRowQueued() =
        component("android.component.queue-row.queued", wide = true) {
            Images {
                AndroidQueueRow(
                    title = "Margins",
                    artist = "Mara Quill",
                    cover = playing.cover,
                    album = "Northbound",
                    duration = "4:55",
                )
            }
        }

    @Test
    fun androidComponentQueueRowNowPlaying() =
        component("android.component.queue-row.now-playing", wide = true) {
            Images {
                AndroidQueueRow(title = "Low Tide Hours", artist = "Mara Quill", cover = playing.cover, current = true)
            }
        }

    @Test
    fun androidComponentQueuePlaceholderRowNotLoaded() =
        component("android.component.queue-placeholder-row.not-loaded", wide = true) { AndroidQueuePlaceholderRow() }

    @Test
    fun androidComponentQueuePlaceholderRowBesideALoadedRow() =
        component("android.component.queue-placeholder-row.beside-a-loaded-row", wide = true) {
            Images {
                Column {
                    AndroidQueueRow(
                        title = "Margins",
                        artist = "Mara Quill",
                        cover = playing.cover,
                        album = "Northbound",
                        duration = "4:55",
                    )
                    AndroidQueuePlaceholderRow()
                }
            }
        }

    @Test
    fun androidComponentQueueSectionUpNext() =
        component("android.component.queue-section.up-next", wide = true) { AndroidQueueSection("Up next", "Clear up next", {}) }

    @Test
    fun androidComponentQueueSectionPlayingFrom() =
        component("android.component.queue-section.playing-from", wide = true) {
            AndroidQueueSection("Playing from", "Clear playing from", {}, shuffled = false)
        }
}

@Composable
internal fun Images(content: @Composable () -> Unit) =
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved(), content = content)

@Composable
internal fun Bar(
    staging: PlayerStaging,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
    castButton: @Composable () -> Unit = { CastBarButton(casting = false, onClick = {}) },
    failure: DisplayError? = null,
) = Images {
    AndroidNowPlayingBar(
        track = if (failure == null) staging.track else null,
        failure = failure,
        onExpand = {},
        transport = {
            NowPlayingBarControls(
                isPlaying = true,
                waitsOnStorage = staging.waitsOnStorage,
                repeatMode = repeatMode,
                onPrevious = {},
                onPlayPause = {},
                onNext = {},
                castButton = castButton,
                onOpenQueue = {},
                onCycleRepeat = {},
            )
        },
        progress = { staging.SeekRow() },
        castEnded = {},
    )
}

/** The full player as the boards draw it: [staging]'s track, playing or not, at [volume], with repeat set to [repeatMode]. */
@Composable
internal fun FullPlayer(
    staging: PlayerStaging,
    isPlaying: Boolean = true,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
    volume: Float = 0.7f,
    receiverWithoutVolume: String? = null,
) = ScreenScene {
    Images {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            AndroidFullPlayer(
                track = staging.track,
                failure = null,
                onCollapse = {},
                seek = { staging.SeekRow(SeekLineLook.LARGE) },
                transport = {
                    AndroidTransport(isPlaying, staging.waitsOnStorage, onPrevious = {}, onPlayPause = {}, onNext = {}, full = true)
                },
                secondaryControls = { ExpandedSecondaryControls(repeatMode, onCycleRepeat = {}) },
                volumeRow = { AndroidVolumeSlider(volume, false, {}, {}, {}, receiverWithoutVolume) },
            )
        }
    }
}

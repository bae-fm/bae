package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.AndroidNowPlayingBar
import fm.bae.app.ui.playback.AndroidPlayPause
import fm.bae.app.ui.playback.AndroidQueueLabel
import fm.bae.app.ui.playback.AndroidQueuePlaceholderRow
import fm.bae.app.ui.playback.AndroidQueueRow
import fm.bae.app.ui.playback.CastBarButton
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.ExpandedTransportRow
import fm.bae.app.ui.playback.ExpandedVolumeRow
import fm.bae.app.ui.playback.NowPlayingTransport
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackSpan

internal val playing =
    PlayerStaging(progress = 0.35, elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)))
private val waiting =
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
        component("android.component.play-pause.paused") { AndroidPlayPause(false, ThemeIcon.large, null) {} }

    @Test
    fun androidComponentPlayPausePlaying() =
        component("android.component.play-pause.playing") { AndroidPlayPause(true, ThemeIcon.large, null) {} }

    @Test
    fun androidComponentPlayPauseWaiting() =
        component("android.component.play-pause.waiting") { AndroidPlayPause(true, ThemeIcon.large, waiting.storageWait) {} }

    @Test
    fun androidComponentPlayPauseFullPlayer() =
        component("android.component.play-pause.full-player") { AndroidPlayPause(true, 48.dp, null) {} }

    @Test
    fun androidComponentSeekBarPlaying() = component("android.component.seek-bar.playing", wide = true) { playing.SeekRow() }

    @Test
    fun androidComponentSeekBarHoles() = component("android.component.seek-bar.holes", wide = true) { holes.SeekRow() }

    @Test
    fun androidComponentFullPlayerDefault() =
        shot("android.component.full-player.default") {
            ScreenScene {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                        AndroidFullPlayer(
                            track = playing.track,
                            failure = null,
                            onCollapse = {},
                            seek = { playing.SeekRow(SeekLineLook.LARGE) },
                            transport = {
                                ExpandedTransportRow(true, playing.storageWait, onPrevious = {}, onPlayPause = {}, onNext = {})
                            },
                            secondaryControls = { ExpandedSecondaryControls(BridgeRepeatMode.OFF, onCycleRepeat = {}) },
                            volumeRow = { ExpandedVolumeRow(0.7f, false, {}, {}, {}) },
                        )
                    }
                }
            }
        }

    @Test
    fun androidComponentQueueRowQueued() =
        component("android.component.queue-row.queued", wide = true) {
            Images {
                AndroidQueueRow(
                    title = "Margins",
                    artist = "Mara Quill",
                    cover = playing.track.coverImage,
                    album = "Northbound",
                    duration = "4:55",
                )
            }
        }

    @Test
    fun androidComponentQueueRowNowPlaying() =
        component("android.component.queue-row.now-playing", wide = true) {
            Images {
                AndroidQueueRow(title = "Low Tide Hours", artist = "Mara Quill", cover = playing.track.coverImage, current = true)
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
                        cover = playing.track.coverImage,
                        album = "Northbound",
                        duration = "4:55",
                    )
                    AndroidQueuePlaceholderRow()
                }
            }
        }

    @Test
    fun androidComponentQueueLabelUpNext() =
        component("android.component.queue-label.up-next", wide = true) { AndroidQueueLabel("Up next", "Clear up next", {}) }

    @Test
    fun androidComponentQueueLabelPlayingFrom() =
        component("android.component.queue-label.playing-from", wide = true) {
            AndroidQueueLabel("Playing from", "Clear playing from", {}, shuffled = false)
        }
}

@Composable
internal fun Images(content: @Composable () -> Unit) =
    CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved(), content = content)

@Composable
internal fun Bar(staging: PlayerStaging) =
    Images {
        AndroidNowPlayingBar(
            track = staging.track,
            failure = null,
            onExpand = {},
            transport = {
                NowPlayingTransport(
                    isPlaying = true,
                    storageWait = staging.storageWait,
                    repeatMode = BridgeRepeatMode.OFF,
                    onPrevious = {},
                    onPlayPause = {},
                    onNext = {},
                    castButton = { CastBarButton(casting = false, onClick = {}) },
                    onOpenQueue = {},
                    onCycleRepeat = {},
                )
            },
            progress = { staging.SeekRow() },
            castEnded = {},
        )
    }

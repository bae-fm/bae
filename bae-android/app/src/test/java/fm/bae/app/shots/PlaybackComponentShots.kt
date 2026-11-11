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
import fm.bae.app.ui.playback.AndroidQueueRow
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.ExpandedTransportRow
import fm.bae.app.ui.playback.ExpandedVolumeRow
import fm.bae.app.ui.playback.NowPlayingTransport
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode

internal val playing = PlayerStaging()

/** The component shots of the player: the bar, its play and pause, the seek row, the full player and the queue's rows. */
class PlaybackComponentShots : BoardShot() {
    @Test
    fun androidComponentNowPlayingBarPlaying() = component("android.component.now-playing-bar.playing", wide = true) { Bar(playing) }

    @Test
    fun androidComponentPlayPausePaused() =
        component("android.component.play-pause.paused") { AndroidPlayPause(false, ThemeIcon.large, null) {} }

    @Test
    fun androidComponentPlayPausePlaying() =
        component("android.component.play-pause.playing") { AndroidPlayPause(true, ThemeIcon.large, null) {} }

    @Test
    fun androidComponentPlayPauseFullPlayer() =
        component("android.component.play-pause.full-player") { AndroidPlayPause(true, 48.dp, null) {} }

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
                            seek = { playing.SeekRow() },
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
    fun androidComponentQueueLabelUpNext() =
        component("android.component.queue-label.up-next", wide = true) { AndroidQueueLabel("Up Next", "Clear Up Next", {}) }

    @Test
    fun androidComponentQueueLabelPlayingFrom() =
        component("android.component.queue-label.playing-from", wide = true) {
            AndroidQueueLabel("Playing From", "Clear Playing From", {}, shuffled = false)
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
                    castButton = {},
                    onOpenQueue = {},
                    onCycleRepeat = {},
                )
            },
            progress = { staging.SeekRow() },
            castEnded = {},
        )
    }

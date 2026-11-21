package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.DisplayError
import fm.bae.app.ui.playback.AndroidNowPlayingBar
import fm.bae.app.ui.playback.CastBarButton
import fm.bae.app.ui.playback.NowPlayingBarControls
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode

/** The component shots of `AndroidNowPlayingBar`, one for each row of its preview. */
class AndroidNowPlayingBarShots : BoardShot() {
    @Test
    fun androidComponentNowPlayingBarPlaying() = component("android.component.now-playing-bar.playing") { Bar(playing) }

    @Test
    fun androidComponentNowPlayingBarWaitingOnTheCloud() =
        component("android.component.now-playing-bar.waiting-on-the-cloud") { Bar(waiting) }

    @Test
    fun androidComponentNowPlayingBarNoCover() = component("android.component.now-playing-bar.no-cover") { Bar(withoutCover) }

    @Test
    fun androidComponentNowPlayingBarCasting() =
        component("android.component.now-playing-bar.casting") {
            Bar(playing, castButton = { CastBarButton(casting = true, onClick = {}) })
        }

    @Test
    fun androidComponentNowPlayingBarCastOff() = component("android.component.now-playing-bar.cast-off") { Bar(playing, castButton = {}) }

    @Test
    fun androidComponentNowPlayingBarRepeat() =
        component("android.component.now-playing-bar.repeat") { Bar(playing, repeatMode = BridgeRepeatMode.CONTEXT) }

    @Test
    fun androidComponentNowPlayingBarError() =
        component("android.component.now-playing-bar.error") {
            Bar(playing, failure = DisplayError("Could not read what is playing."))
        }
}

@Composable
internal fun Bar(
    staging: PlayerStaging,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
    castButton: @Composable () -> Unit = { CastBarButton(casting = false, onClick = {}) },
    failure: DisplayError? = null,
) = Covers {
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

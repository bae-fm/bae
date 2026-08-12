package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

/**
 * The repeat-mode mapping in both directions. Core reports its mode as a
 * [BridgeRepeatMode]; the player mirrors it into the Media3 [Player] repeat mode
 * (for the system controls) and the in-app [BaeCorePlayer.repeatMode] flow.
 * A user toggling the system control drives the reverse: the Media3 repeat mode
 * maps back to a [BridgeRepeatMode] the player forwards to core.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class RepeatModeMappingTest {
    @Test
    fun coreModeMapsToMedia3AndInAppFlow() {
        val player = player()

        val cases =
            listOf(
                BridgeRepeatMode.OFF to Player.REPEAT_MODE_OFF,
                BridgeRepeatMode.TRACK to Player.REPEAT_MODE_ONE,
                BridgeRepeatMode.CONTEXT to Player.REPEAT_MODE_ALL,
            )
        for ((coreMode, media3Mode) in cases) {
            player.applyValues(playbackValues(BridgePlaybackValueState.Stopped, repeatMode = coreMode))
            shadowOf(Looper.getMainLooper()).idle()

            assertEquals(coreMode, player.repeatMode.value)
            assertEquals(media3Mode, (player as Player).repeatMode)
        }
    }

    @Test
    fun userMedia3ModeMapsBackToCoreMode() {
        val (player, handle) = playerWithRecorder()

        val cases =
            listOf(
                Player.REPEAT_MODE_OFF to BridgeRepeatMode.OFF,
                Player.REPEAT_MODE_ONE to BridgeRepeatMode.TRACK,
                Player.REPEAT_MODE_ALL to BridgeRepeatMode.CONTEXT,
            )
        for ((media3Mode, coreMode) in cases) {
            handle.repeatModes.clear()
            (player as Player).repeatMode = media3Mode
            shadowOf(Looper.getMainLooper()).idle()

            assertEquals(coreMode, handle.repeatModes.single())
        }
    }

    private fun player(): BaeCorePlayer = player(FakeAppHandle())

    private fun playerWithRecorder(): Pair<BaeCorePlayer, RepeatModeRecordingHandle> {
        val handle = RepeatModeRecordingHandle()
        return player(handle) to handle
    }

    private fun player(handle: AppHandle): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            context = RuntimeEnvironment.getApplication(),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )

    private class RepeatModeRecordingHandle : AppHandle(NoHandle) {
        val repeatModes = mutableListOf<BridgeRepeatMode>()

        override fun setRepeatMode(mode: BridgeRepeatMode) {
            repeatModes += mode
        }

        override suspend fun savePlaybackState() {}

        override fun subscribeUiEvents(callback: UiEventCallback) {}
    }
}

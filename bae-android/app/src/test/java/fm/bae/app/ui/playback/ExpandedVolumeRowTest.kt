package fm.bae.app.ui.playback

import android.app.Application
import android.view.View
import android.view.ViewGroup
import android.widget.SeekBar
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.data.receiverWithoutVolume
import fm.bae.app.ui.TestTheme
import kotlinx.coroutines.CompletableDeferred
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeRemotePlayback
import uniffi.bae_bridge.BridgeRendererKind

/** The volume row: mute and the slider, which shows a dragged level until the call that sets it returns, or for a receiver that takes no volume from bae, a line saying where it is set. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ExpandedVolumeRowTest {
    @get:Rule
    val compose = createComposeRule()

    private fun receiver(takesVolume: Boolean) =
        BridgeCastReceiver(
            id = "den",
            name = "Den",
            kind = BridgeRendererKind.DLNA,
            model = null,
            address = "192.168.1.57",
            takesVolume = takesVolume,
        )

    private fun show(receiverWithoutVolume: String?) {
        compose.setContent {
            TestTheme {
                ExpandedVolumeRow(0.7f, false, {}, {}, {}, receiverWithoutVolume)
            }
        }
    }

    @Test
    fun aReceiverWithoutVolumeGetsALineInPlaceOfMuteAndTheSlider() {
        show(receiverWithoutVolume(BridgeRemotePlayback.On(receiver(takesVolume = false))))

        compose.onNodeWithText("Volume is set on Den").assertIsDisplayed()
        compose.onAllNodesWithContentDescription("Mute").assertCountEquals(0)
    }

    @Test
    fun otherPlaybackKeepsMuteAndTheSlider() {
        assertEquals(null, receiverWithoutVolume(BridgeRemotePlayback.On(receiver(takesVolume = true))))
        assertEquals(null, receiverWithoutVolume(BridgeRemotePlayback.Local))
        show(null)

        compose.onNodeWithContentDescription("Mute").assertIsDisplayed()
        compose.onAllNodesWithText("Volume is set on Den").assertCountEquals(0)
    }

    /** The slider shows the level a drag ended on until the call that sets it returns, then the stored level. */
    @Test
    fun aDraggedLevelStaysUntilItsSetReturns() {
        val stored = CompletableDeferred<Unit>()
        val sets = mutableListOf<Float>()
        lateinit var root: View
        compose.setContent {
            root = LocalView.current
            TestTheme {
                ExpandedVolumeRow(0.7f, false, {}, {}, { level ->
                    sets.add(level)
                    stored.await()
                })
            }
        }
        compose.waitForIdle()
        val bar = checkNotNull(seekBarIn(root)) { "the volume row draws a seek bar" }
        val listener = shadowOf(bar).onSeekBarChangeListener

        compose.runOnUiThread {
            listener.onStartTrackingTouch(bar)
            bar.progress = 300
            listener.onProgressChanged(bar, 300, true)
            listener.onStopTrackingTouch(bar)
        }
        compose.waitForIdle()
        assertEquals(listOf(0.3f), sets)
        assertEquals("the dragged level shows until the call that sets it returns", 300, bar.progress)

        stored.complete(Unit)
        compose.waitForIdle()
        assertEquals("the stored level shows once the call that sets it returns", 700, bar.progress)
    }

    private fun seekBarIn(view: View): SeekBar? =
        when (view) {
            is SeekBar -> view
            is ViewGroup -> (0 until view.childCount).firstNotNullOfOrNull { seekBarIn(view.getChildAt(it)) }
            else -> null
        }
}

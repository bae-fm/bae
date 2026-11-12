package fm.bae.app.ui.playback

import android.app.Application
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cast
import androidx.compose.material3.Icon
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.PreviewTheme
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeTrackPosition

/**
 * The bar's controls are drawn smaller than Android's 48 dp touch target and still take a press
 * out to it: the 40 dp buttons, and the seek line, whose view is 48 dp tall around the 14 dp row
 * it draws.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class TouchTargetTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aPressJustOutsideABarButtonPresses() {
        var presses = 0
        compose.setContent {
            PreviewTheme {
                Box(modifier = Modifier.size(200.dp), contentAlignment = Alignment.Center) {
                    BarIconButton(onClick = { presses++ }) { Icon(Icons.Filled.Cast, contentDescription = null) }
                }
            }
        }

        // The 40 dp button spans 80 to 120 dp across the box; 3 dp past its right edge.
        compose.onRoot().performTouchInput { click(Offset(123.dp.toPx(), 100.dp.toPx())) }

        assertEquals(1, presses)
    }

    @Test
    fun aPressAboveTheSeekLinesDrawnRowMovesThePosition() {
        val positions = LatestStream.written<BridgeTrackPosition>()
        positions.write(KEY, BridgeTrackPosition(10_000, DURATION_MS.toULong(), 0.1, 0uL))
        val ratios = mutableListOf<Double>()
        compose.setContent {
            PreviewTheme {
                // The row's drawn line sits 4 dp under its top and is 14 dp tall: 104 to 118 dp.
                Box(modifier = Modifier.padding(top = 100.dp)) {
                    AndroidSeekBar(
                        position = positions.source(KEY),
                        buffered = null,
                        state = { SeekBarState(it?.progress ?: 0.0, "", "", seekable = true) },
                        onSeekRatio = { ratios += it },
                        onToggleRemaining = {},
                    )
                }
            }
        }

        // 12 dp above the drawn row, inside the view's 48 dp, and half way along it.
        compose.onRoot().performTouchInput { click(Offset(width / 2f, 92.dp.toPx())) }

        assertEquals(1, ratios.size)
        assertTrue("the press landed at ${ratios.single()}", ratios.single() in 0.3..0.7)
    }

    private companion object {
        const val KEY = "track"
        const val DURATION_MS = 100_000L
    }
}

package fm.bae.app.ui.appearance

import android.graphics.drawable.ColorDrawable
import android.view.Window
import androidx.activity.ComponentActivity
import androidx.compose.material3.Text
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.core.view.WindowCompat
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [34])
class AppearanceWindowTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theWindowAndItsBarsFollowTheAppModeAndTone() {
        val initial = AppearancePreferences(mode = AppearanceMode.DARK, tone = SurfaceTone.PLUM)
        val store = AppearanceStore(initial) {}
        compose.setContent { BaeTheme(store) { Text("Appearance") } }
        compose.runOnIdle {
            val window = compose.activity.window
            assertEquals(appearanceColorScheme(initial, dark = true).background.toArgb(), window.backgroundColor())
            val bars = WindowCompat.getInsetsController(window, window.decorView)
            assertFalse(bars.isAppearanceLightStatusBars)
            assertFalse(bars.isAppearanceLightNavigationBars)
        }
        compose.runOnIdle { runBlocking { store.setMode(AppearanceMode.LIGHT) } }
        compose.runOnIdle {
            val window = compose.activity.window
            val expected = appearanceColorScheme(store.preferences.value, dark = false).background.toArgb()
            assertEquals(expected, window.backgroundColor())
            val bars = WindowCompat.getInsetsController(window, window.decorView)
            assertTrue(bars.isAppearanceLightStatusBars)
            assertTrue(bars.isAppearanceLightNavigationBars)
        }
    }
}

/** The colour the window draws under the app, which shows through its clear bars. */
private fun Window.backgroundColor(): Int = (decorView.background as ColorDrawable).color

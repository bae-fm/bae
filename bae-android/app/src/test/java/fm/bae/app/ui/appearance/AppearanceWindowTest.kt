package fm.bae.app.ui.appearance

import androidx.activity.ComponentActivity
import androidx.compose.material3.Text
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.view.WindowCompat
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AppearanceWindowTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun systemBarsFollowTheAppModeAndTone() {
        val initial = AppearancePreferences(mode = AppearanceMode.DARK, tone = SurfaceTone.PLUM)
        val store = AppearanceStore(initial) {}
        compose.setContent { BaeTheme(store) { Text("Appearance") } }
        compose.runOnIdle {
            val window = compose.activity.window
            val palette = AppearancePalette(compose.activity.resources.openRawResource(R.raw.appearance_palette))
            val expected = palette.colors(initial, dark = true).background.toArgb()
            assertEquals(expected, window.statusBarColor)
            assertEquals(expected, window.navigationBarColor)
            assertFalse(WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars)
        }
        compose.runOnIdle { runBlocking { store.setMode(AppearanceMode.LIGHT) } }
        compose.runOnIdle {
            val window = compose.activity.window
            val palette = AppearancePalette(compose.activity.resources.openRawResource(R.raw.appearance_palette))
            val expected = palette.colors(store.preferences.value, dark = false).background.toArgb()
            assertEquals(expected, window.statusBarColor)
            assertEquals(expected, window.navigationBarColor)
            assertTrue(WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars)
        }
    }
}

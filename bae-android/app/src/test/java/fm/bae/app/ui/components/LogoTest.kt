package fm.bae.app.ui.components

import android.app.Application
import android.os.Looper
import android.provider.Settings
import android.view.View
import androidx.activity.ComponentActivity
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.core.view.drawToBitmap
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.setTestAnimatorScale
import fm.bae.app.setTestBatterySaver
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LogoTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun theVisibleLogoStartsMovingWhenAndroidEnablesAnimations() {
        setTestAnimatorScale(0f)
        showLogo()
        val rest = drawLogo()

        setTestAnimatorScale(1f)
        hearTheChange()
        compose.mainClock.advanceTimeBy(MOVING_FRAME_MILLIS)

        assertFalse(rest.sameAs(drawLogo()))
    }

    @Test
    @Config(sdk = [32], application = Application::class)
    fun theVisibleLogoPausesAndResumesWithBatterySaver() {
        val context = compose.activity
        Settings.Global.putFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
        setTestBatterySaver(context, true)
        showLogo()
        val rest = drawLogo()

        setTestBatterySaver(context, false)
        hearTheChange()
        compose.mainClock.advanceTimeBy(MOVING_FRAME_MILLIS)
        assertFalse(rest.sameAs(drawLogo()))

        setTestBatterySaver(context, true)
        hearTheChange()
        compose.mainClock.advanceTimeByFrame()
        assertTrue(rest.sameAs(drawLogo()))
    }

    private fun showLogo() {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            TestTheme { Logo(size = 120.dp, bob = true, glow = true) }
        }
    }

    /**
     * Delivers the change to the logo, which writes it outside a composition. The compose runtime hears of such a write
     * only once it is told, and after some other test classes in the same process, nothing else tells it.
     */
    private fun hearTheChange() {
        shadowOf(Looper.getMainLooper()).idle()
        Snapshot.sendApplyNotifications()
    }

    private fun drawLogo() = compose.runOnIdle { compose.activity.findViewById<View>(android.R.id.content).drawToBitmap() }

    private companion object {
        const val MOVING_FRAME_MILLIS = 600L
    }
}

package fm.bae.app.ui.components

import android.app.Activity
import android.app.Application
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.graphics.Color
import androidx.core.view.drawToBitmap
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.setTestAnimatorScale
import fm.bae.app.setTestBatterySaver
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowChoreographer
import org.robolectric.shadows.ShadowLooper
import org.robolectric.shadows.ShadowSystemClock
import java.time.Duration
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidProgressBarViewTest {
    @Test
    fun aKeyWithoutProgressReportsNoPercentage() {
        val bar = AndroidProgressBarView(RuntimeEnvironment.getApplication())
        bar.follow(LatestStream.written<Double>().source("transfer"))
        val info = AccessibilityNodeInfo()
        bar.onInitializeAccessibilityNodeInfo(info)
        assertNull(info.rangeInfo)
    }

    @Test
    @Config(sdk = [32], application = Application::class)
    fun aHiddenBarSchedulesNoAnimationFrames() {
        Robolectric.buildActivity(Activity::class.java).setup().use { activity ->
            val bar = AndroidProgressBarView(activity.get()).apply { visibility = View.GONE }
            activity.get().setContentView(bar)
            val main = shadowOf(Looper.getMainLooper())
            attach(bar, main)
            setTestAnimatorScale(1f)
            while (ShadowChoreographer.getNextVsyncTimeNanos() != 0L) drawNextFrame(main)

            bar.showMoving()
            assertEquals(0L, ShadowChoreographer.getNextVsyncTimeNanos())
        }
    }

    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anAttachedBarFollowsAnimatorScaleChangesFromOffTheMainThread() {
        Robolectric.buildActivity(Activity::class.java).setup().use { activity ->
            val bar = AndroidProgressBarView(activity.get())
            bar.applyAccent(Color.Red)
            activity.get().setContentView(bar)
            val main = shadowOf(Looper.getMainLooper())
            attach(bar, main)

            setTestAnimatorScale(0f)
            bar.showMoving()
            val rest = bar.drawToBitmap()

            changeScaleOffMain(1f)
            drawNextFrame(main)
            drawNextFrame(main)
            assertFalse(rest.sameAs(bar.drawToBitmap()))

            changeScaleOffMain(0f)
            main.idle()
            assertTrue(rest.sameAs(bar.drawToBitmap()))
        }
    }

    @Test
    @Config(sdk = [32], application = Application::class)
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anOlderAndroidBarResumesWhenTheSettingArrivesBeforeTheAnimatorScale() {
        Robolectric.buildActivity(Activity::class.java).setup().use { activity ->
            val bar = AndroidProgressBarView(activity.get())
            bar.applyAccent(Color.Red)
            activity.get().setContentView(bar)
            val main = shadowOf(Looper.getMainLooper())
            attach(bar, main)
            val resolver = activity.get().contentResolver

            setTestAnimatorScale(0f)
            Settings.Global.putFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 0f)
            bar.showMoving()
            val rest = bar.drawToBitmap()

            Settings.Global.putFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)
            // The Robolectric settings shadow applies both changes at once. Deliver the system's scale later.
            setTestAnimatorScale(0f)
            main.idle()
            setTestAnimatorScale(1f)
            drawNextFrame(main)
            drawNextFrame(main)
            drawNextFrame(main)

            assertFalse(rest.sameAs(bar.drawToBitmap()))
        }
    }

    @Test
    @Config(sdk = [32], application = Application::class)
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anOlderAndroidBarFollowsBatterySaverChanges() {
        Robolectric.buildActivity(Activity::class.java).setup().use { controller ->
            val activity = controller.get()
            val bar = AndroidProgressBarView(activity)
            bar.applyAccent(Color.Red)
            activity.setContentView(bar)
            val main = shadowOf(Looper.getMainLooper())
            attach(bar, main)
            Settings.Global.putFloat(activity.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f)

            setTestBatterySaver(activity, true)
            bar.showMoving()
            val rest = bar.drawToBitmap()
            drawNextFrame(main)
            drawNextFrame(main)
            assertTrue(rest.sameAs(bar.drawToBitmap()))

            setTestBatterySaver(activity, false)
            drawNextFrame(main)
            drawNextFrame(main)
            assertFalse(rest.sameAs(bar.drawToBitmap()))

            setTestBatterySaver(activity, true)
            main.idle()
            assertTrue(rest.sameAs(bar.drawToBitmap()))
        }
    }

    private fun attach(
        bar: AndroidProgressBarView,
        main: ShadowLooper,
    ) {
        drawNextFrame(main)
        assertTrue(bar.isAttachedToWindow)
    }

    private fun drawNextFrame(main: ShadowLooper) {
        main.idle()
        val nextFrame = ShadowChoreographer.getNextVsyncTimeNanos()
        if (nextFrame == 0L) return
        val uptime = TimeUnit.MILLISECONDS.toNanos(SystemClock.uptimeMillis())
        ShadowSystemClock.advanceBy(Duration.ofNanos((nextFrame - uptime).coerceAtLeast(0L)))
        main.idle()
    }

    private fun changeScaleOffMain(scale: Float) {
        val change = FutureTask { setTestAnimatorScale(scale) }
        Thread(change).start()
        change.get(5, TimeUnit.SECONDS)
    }
}

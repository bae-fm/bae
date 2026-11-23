package fm.bae.app.ui.components

import android.app.Activity
import android.app.Application
import android.os.Looper
import android.view.accessibility.AccessibilityNodeInfo
import androidx.compose.ui.graphics.Color
import androidx.core.view.drawToBitmap
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.setTestAnimatorScale
import fm.bae.fastvalues.LatestStream
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
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun anAttachedBarFollowsAnimatorScaleChangesFromOffTheMainThread() {
        Robolectric.buildActivity(Activity::class.java).setup().use { activity ->
            val bar = AndroidProgressBarView(activity.get())
            bar.applyAccent(Color.Red)
            activity.get().setContentView(bar)
            val main = shadowOf(Looper.getMainLooper())
            main.idleFor(Duration.ofMillis(160))
            assertTrue(bar.isAttachedToWindow)

            setTestAnimatorScale(0f)
            bar.showMoving()
            val rest = bar.drawToBitmap()

            changeScaleOffMain(1f)
            main.idleFor(Duration.ofMillis(160))
            assertFalse(rest.sameAs(bar.drawToBitmap()))

            changeScaleOffMain(0f)
            main.idleFor(Duration.ofMillis(160))
            assertTrue(rest.sameAs(bar.drawToBitmap()))
        }
    }

    private fun changeScaleOffMain(scale: Float) {
        val change = FutureTask { setTestAnimatorScale(scale) }
        Thread(change).start()
        change.get(5, TimeUnit.SECONDS)
    }
}

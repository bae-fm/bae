package fm.bae.app.ui.components

import android.app.Application
import android.view.accessibility.AccessibilityNodeInfo
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

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
}

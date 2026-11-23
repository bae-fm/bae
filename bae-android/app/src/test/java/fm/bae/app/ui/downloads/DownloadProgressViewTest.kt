package fm.bae.app.ui.downloads

import android.app.Application
import android.view.accessibility.AccessibilityNodeInfo
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.fastvalues.LatestStream
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDownloadTransferProgress

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class DownloadProgressViewTest {
    @Test
    fun aPinWithoutAByteMeasurementReportsNoPercentage() {
        val download = DownloadProgressView(RuntimeEnvironment.getApplication())
        download.follow(LatestStream.written<BridgeDownloadTransferProgress>().source("release"))
        val info = AccessibilityNodeInfo()
        download.getChildAt(0).onInitializeAccessibilityNodeInfo(info)
        assertNull(info.rangeInfo)
    }
}

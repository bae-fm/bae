package fm.bae.app

import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog
import java.net.UnknownHostException

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BaeLoggerTest {
    /** A DNS failure keeps its trace: Log.getStackTraceString prints nothing for a chain holding one. */
    @Test
    fun aChainHoldingAnUnknownHostLogsItsTrace() {
        val failure = IllegalStateException("sync failed", UnknownHostException("bucket.example"))

        BaeLogger("bae.BaeLoggerTest").error("upload failed", failure)

        val logged = ShadowLog.getLogsForTag("bae.BaeLoggerTest").single().msg
        assertTrue(logged, logged.contains("UnknownHostException: bucket.example"))
        assertTrue(logged, logged.contains("IllegalStateException: sync failed"))
    }
}

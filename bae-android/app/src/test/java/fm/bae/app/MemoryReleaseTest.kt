package fm.bae.app

import android.app.Application
import android.content.ComponentCallbacks2
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * The open library gives back its store's cached memory each time Android
 * asks the app to trim, at every level, including when bae leaves the screen
 * and while it waits in the background, and stops once its session does.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class MemoryReleaseTest {
    private val application: Application = RuntimeEnvironment.getApplication()

    @Test
    fun everyTrimLevelAndLowMemoryWarningReleasesTheStoresMemory() =
        runTest(StandardTestDispatcher()) {
            var releases = 0
            val memory = MemoryRelease(application, backgroundScope) { releases++ }
            memory.start()
            val levels =
                listOf(
                    ComponentCallbacks2.TRIM_MEMORY_RUNNING_MODERATE,
                    ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW,
                    ComponentCallbacks2.TRIM_MEMORY_RUNNING_CRITICAL,
                    ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN,
                    ComponentCallbacks2.TRIM_MEMORY_BACKGROUND,
                    ComponentCallbacks2.TRIM_MEMORY_MODERATE,
                    ComponentCallbacks2.TRIM_MEMORY_COMPLETE,
                )
            levels.forEachIndexed { index, level ->
                application.onTrimMemory(level)
                runCurrent()
                assertEquals("level $level", index + 1, releases)
            }
            @Suppress("DEPRECATION")
            application.onLowMemory()
            runCurrent()
            assertEquals("a low-memory warning", levels.size + 1, releases)
            memory.stop()
        }

    @Test
    fun stoppingCancelsARunningReleaseAndHearsNoMoreTrims() =
        runTest(StandardTestDispatcher()) {
            var releases = 0
            val held = CompletableDeferred<Unit>()
            var finished = false
            val memory =
                MemoryRelease(application, backgroundScope) {
                    releases++
                    held.await()
                    finished = true
                }
            memory.start()
            application.onTrimMemory(ComponentCallbacks2.TRIM_MEMORY_UI_HIDDEN)
            runCurrent()
            assertEquals(1, releases)
            memory.stop()
            held.complete(Unit)
            runCurrent()
            assertFalse("the release still running was cancelled", finished)
            application.onTrimMemory(ComponentCallbacks2.TRIM_MEMORY_BACKGROUND)
            runCurrent()
            assertEquals(1, releases)
        }

    @Test
    fun stoppingWhatNeverStartedDoesNothing() =
        runTest(StandardTestDispatcher()) {
            var releases = 0
            MemoryRelease(application, backgroundScope) { releases++ }.stop()
            application.onTrimMemory(ComponentCallbacks2.TRIM_MEMORY_BACKGROUND)
            runCurrent()
            assertEquals(0, releases)
        }
}

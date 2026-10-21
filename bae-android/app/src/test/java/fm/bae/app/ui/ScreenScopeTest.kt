package fm.bae.app.ui

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * A screen's coroutine that fails in a way nothing caught is a bug: reported,
 * with the app going on, and without ending the screen's other work.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class ScreenScopeTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aFailingLaunchIsReportedAndLeavesTheScopeWorking() {
        val bugs = BugReports()
        lateinit var scope: CoroutineScope
        compose.setContent {
            CompositionLocalProvider(LocalBugReports provides bugs) {
                scope = rememberScreenScope()
            }
        }

        scope.launch { error("a call that can't fail failed") }
        compose.waitForIdle()
        runBlocking { withTimeout(1_000) { bugs.reported.first() } }

        var ranAfter = false
        scope.launch { ranAfter = true }
        compose.waitForIdle()
        assertTrue("the scope still runs work after a bug", ranAfter)
    }
}

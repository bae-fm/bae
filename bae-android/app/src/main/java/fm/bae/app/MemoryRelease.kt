package fm.bae.app

import android.content.ComponentCallbacks2
import android.content.Context
import android.content.res.Configuration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.job
import kotlinx.coroutines.launch

/**
 * Gives back the memory the open library's store keeps cached each time
 * Android warns of low memory or asks bae to trim its memory. `onTrimMemory`
 * asks at every level it reports: while bae is on screen and the device runs
 * low, when bae leaves the screen (`TRIM_MEMORY_UI_HIDDEN`), and while it waits
 * in the background. Each of those documents giving back what can be rebuilt,
 * and the cached pages come back as reads need them.
 *
 * It lives with one open session: [start] when the session is wired up,
 * [stop] before its handle closes, which also cancels a release still
 * running, so none reaches a closed handle. A release that fails is a bug,
 * which [scope]'s handler reports.
 */
internal class MemoryRelease(
    private val context: Context,
    scope: CoroutineScope,
    private val release: suspend () -> Unit,
) {
    private val releases = SupervisorJob(scope.coroutineContext.job)
    private val releasing = CoroutineScope(scope.coroutineContext + releases)
    private var running = false

    private val trims =
        object : ComponentCallbacks2 {
            override fun onTrimMemory(level: Int) {
                releaseNow()
            }

            // Android's warning before onTrimMemory, which it no longer sends
            // from API 34.
            @Deprecated("Deprecated in Java")
            override fun onLowMemory() {
                releaseNow()
            }

            override fun onConfigurationChanged(newConfig: Configuration) = Unit
        }

    private fun releaseNow() {
        releasing.launch { release() }
    }

    fun start() {
        check(!running) { "memory release already started" }
        running = true
        context.registerComponentCallbacks(trims)
    }

    /** Stop listening and cancel a release in progress; does nothing if never started. */
    fun stop() {
        if (!running) return
        running = false
        context.unregisterComponentCallbacks(trims)
        releases.cancel()
    }
}

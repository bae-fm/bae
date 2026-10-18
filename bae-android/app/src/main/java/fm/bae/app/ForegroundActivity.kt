package fm.bae.app

import android.app.Activity
import java.lang.ref.WeakReference
import java.util.concurrent.CopyOnWriteArrayList

/**
 * The activity bae shows right now, for work that has to present over it —
 * a provider's sign-in page — and a way to hear when bae returns to the
 * front. [MainActivity] reports its own resume and pause; everything here
 * runs on the main thread.
 */
internal object ForegroundActivity {
    private var current: WeakReference<Activity>? = null
    private val resumeListeners = CopyOnWriteArrayList<() -> Unit>()

    /** The resumed activity, or null while bae is in the background. */
    fun get(): Activity? = current?.get()

    fun resumed(activity: Activity) {
        current = WeakReference(activity)
        resumeListeners.forEach { it() }
    }

    fun paused(activity: Activity) {
        if (current?.get() === activity) current = null
    }

    /** Call [listener] each time bae comes back to the front, until the returned function removes it. */
    fun onResume(listener: () -> Unit): () -> Unit {
        resumeListeners.add(listener)
        return { resumeListeners.remove(listener) }
    }
}

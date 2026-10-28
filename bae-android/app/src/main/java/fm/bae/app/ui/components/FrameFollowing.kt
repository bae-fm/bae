package fm.bae.app.ui.components

import android.view.Choreographer
import android.view.View
import fm.bae.app.data.LatestSource

/**
 * A View's reading of one key of a fast stream on the display's own clock:
 * while the key is active and the View is attached, a Choreographer frame
 * callback reads the key's latest value each frame and shows it if it changed;
 * while the key is idle, nothing runs. Values arriving faster than frames are
 * never all shown; the screen shows the latest one each frame.
 */
class FrameFollowing<V : Any>(
    private val view: View,
    private val show: (V?) -> Unit,
) : Choreographer.FrameCallback {
    private var source: LatestSource<V>? = null
    private val changed: () -> Unit = { activityChanged() }
    private var ticking = false
    private var shown: Any? = UNSHOWN

    init {
        view.addOnAttachStateChangeListener(
            object : View.OnAttachStateChangeListener {
                override fun onViewAttachedToWindow(v: View) = activityChanged()

                override fun onViewDetachedFromWindow(v: View) = stopTicking()
            },
        )
    }

    /** Read [source] from now on; the source already read changes nothing. */
    fun follow(source: LatestSource<V>) {
        if (source == this.source) return
        stop()
        this.source = source
        source.store.watch(source.key, changed)
        activityChanged()
    }

    fun stop() {
        source?.let { it.store.stopWatching(it.key, changed) }
        stopTicking()
        source = null
        shown = UNSHOWN
    }

    override fun doFrame(frameTimeNanos: Long) {
        if (!ticking) return
        showLatest()
        if (source?.latest != null && view.isAttachedToWindow) {
            Choreographer.getInstance().postFrameCallback(this)
        } else {
            ticking = false
        }
    }

    /** Start reading each frame as the key starts, and stop as it goes idle, showing what it holds either way. */
    private fun activityChanged() {
        showLatest()
        if (source?.latest != null && view.isAttachedToWindow) {
            if (!ticking) {
                ticking = true
                Choreographer.getInstance().postFrameCallback(this)
            }
        } else {
            stopTicking()
        }
    }

    private fun stopTicking() {
        if (ticking) Choreographer.getInstance().removeFrameCallback(this)
        ticking = false
    }

    private fun showLatest() {
        val latest = source?.latest
        if (shown != UNSHOWN && shown == latest) return
        shown = latest
        show(latest)
    }

    private companion object {
        val UNSHOWN = Any()
    }
}

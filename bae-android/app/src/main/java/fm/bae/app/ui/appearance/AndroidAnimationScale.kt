package fm.bae.app.ui.appearance

import android.animation.ValueAnimator
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.view.Choreographer
import androidx.annotation.MainThread
import androidx.annotation.RequiresApi

/** Reports the current platform setting and its changes on the UI thread until the caller unsubscribes. */
@MainThread
internal fun observeAndroidAnimatorsEnabled(onChange: (Boolean) -> Unit): () -> Unit {
    var active = true
    var previous: Boolean? = null
    val report: () -> Unit = {
        if (active) {
            val enabled = ValueAnimator.areAnimatorsEnabled()
            if (previous != enabled) {
                previous = enabled
                onChange(enabled)
            }
        }
    }
    val unsubscribe =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            observeScaleChanges(report)
        } else {
            // Older Android exposes no listener for the effective scale. The stored setting can change first.
            observeScaleOnFrames(report)
        }
    report()
    return {
        active = false
        unsubscribe()
    }
}

@RequiresApi(Build.VERSION_CODES.TIRAMISU)
private fun observeScaleChanges(report: () -> Unit): () -> Unit {
    val main = Handler(Looper.getMainLooper())
    val callback = Runnable(report)
    val listener = ValueAnimator.DurationScaleChangeListener { main.post(callback) }
    ValueAnimator.registerDurationScaleChangeListener(listener)
    return {
        ValueAnimator.unregisterDurationScaleChangeListener(listener)
        main.removeCallbacks(callback)
    }
}

private fun observeScaleOnFrames(report: () -> Unit): () -> Unit {
    val display = Choreographer.getInstance()
    val callback =
        object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                display.postFrameCallback(this)
                report()
            }
        }
    display.postFrameCallback(callback)
    return { display.removeFrameCallback(callback) }
}

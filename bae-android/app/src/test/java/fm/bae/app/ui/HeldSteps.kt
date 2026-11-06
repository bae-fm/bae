package fm.bae.app.ui

import android.os.Looper
import fm.bae.fastvalues.LatestStream
import org.robolectric.Shadows.shadowOf

/**
 * Write [step] under [key] of a written stream and run only the message that tells the Views following it, never the
 * frames after: a step line reads its stream each frame while the stream holds a step, and Robolectric runs those
 * frames back to back, so a looper left to go idle while a step is held never does. A test writes null before it waits
 * for idle again.
 */
internal fun <V : Any> LatestStream<V>.tellStep(
    key: String,
    step: V?,
) {
    write(key, step)
    shadowOf(Looper.getMainLooper()).runOneTask()
}

package fm.bae.app.playback

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The failures of the live reads [BaeCorePlayer] mirrors: playback values, the
 * queue, and the session's up-next list. Each holds until that read delivers
 * again. While one is set, what the player last showed from it is no longer
 * known to be current, so the now-playing surfaces, or the queue, show the
 * failure in its place: the queue shows a failure of either queue read.
 */
class PlaybackReadFailures {
    private val _values = MutableStateFlow<DisplayError?>(null)
    val values: StateFlow<DisplayError?> = _values.asStateFlow()

    private var queueValue: DisplayError? = null
    private var sessionUpNext: DisplayError? = null
    private val _queue = MutableStateFlow<DisplayError?>(null)
    val queue: StateFlow<DisplayError?> = _queue.asStateFlow()

    fun failValues(error: DisplayError) {
        _values.value = error
    }

    fun failQueue(error: DisplayError) {
        queueValue = error
        showQueue()
    }

    fun failSessionUpNext(error: DisplayError) {
        sessionUpNext = error
        showQueue()
    }

    internal fun valuesArrived() {
        _values.value = null
    }

    internal fun queueArrived() {
        queueValue = null
        showQueue()
    }

    internal fun sessionUpNextArrived() {
        sessionUpNext = null
        showQueue()
    }

    private fun showQueue() {
        _queue.value = queueValue ?: sessionUpNext
    }
}

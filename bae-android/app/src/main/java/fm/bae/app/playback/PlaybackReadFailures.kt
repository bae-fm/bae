package fm.bae.app.playback

import fm.bae.app.DisplayError
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The failures of the two live reads [BaeCorePlayer] mirrors: playback values
 * and the queue. Each holds until that read delivers again. While one is set,
 * what the player last showed from it is no longer known to be current, so the
 * now-playing surfaces, or the queue, show the failure in its place.
 */
class PlaybackReadFailures {
    private val _values = MutableStateFlow<DisplayError?>(null)
    val values: StateFlow<DisplayError?> = _values.asStateFlow()

    private val _queue = MutableStateFlow<DisplayError?>(null)
    val queue: StateFlow<DisplayError?> = _queue.asStateFlow()

    fun failValues(error: DisplayError) {
        _values.value = error
    }

    fun failQueue(error: DisplayError) {
        _queue.value = error
    }

    internal fun valuesArrived() {
        _values.value = null
    }

    internal fun queueArrived() {
        _queue.value = null
    }
}

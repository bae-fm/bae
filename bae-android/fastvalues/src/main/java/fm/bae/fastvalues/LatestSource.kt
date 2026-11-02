package fm.bae.fastvalues

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.channelFlow

/**
 * One key of a fast stream, which a composable hands to the View that draws
 * it. Only FrameFollowing reads its value.
 */
@ConsistentCopyVisibility
data class LatestSource<V : Any> internal constructor(
    internal val stream: LatestStream<V>,
    internal val key: String,
) {
    internal val latest: V? get() = stream.latest(key)
}

/**
 * The key's value each time it changes, for a screen whose value changes now
 * and then, such as where a join stands: while the key holds a value it is
 * read ten times a second and emitted when it changed; while it holds none,
 * nothing runs until it starts again. A value that comes and goes between two
 * reads is never seen.
 */
fun <V : Any> LatestSource<V>.changes(): Flow<V?> =
    channelFlow {
        val started = Channel<Unit>(Channel.CONFLATED)
        val changed = { started.trySend(Unit).let { } }
        stream.activity.watch(key, changed)
        try {
            var shown = latest
            send(shown)
            while (true) {
                if (shown == null) started.receive() else delay(READ_EVERY_MS)
                val now = latest
                if (now != shown) {
                    shown = now
                    send(now)
                }
            }
        } finally {
            stream.activity.stopWatching(key, changed)
        }
    }

/** How often [changes] reads a key that holds a value. */
private const val READ_EVERY_MS = 100L

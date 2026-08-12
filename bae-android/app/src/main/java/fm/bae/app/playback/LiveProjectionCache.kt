package fm.bae.app.playback

import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import uniffi.bae_bridge.BridgeException
import java.util.LinkedHashMap

internal class LiveProjectionCache<Key, Value>(
    private val scope: CoroutineScope,
    private val maximumCount: Int,
    private val flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    private val onEvent: (Key, LiveQueryEvent<Value>, isInitial: Boolean) -> Unit,
) {
    private class Entry<Value>(
        val identity: Any = Any(),
        var waiters: Int = 0,
        var latest: LiveQueryEvent<Value>? = null,
        var changed: CompletableDeferred<Unit> = CompletableDeferred(),
        var job: Job? = null,
        var retired: BridgeException? = null,
    )

    private val lock = Any()
    private val entries = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun value(key: Key): Value =
        when (val event = event(key)) {
            is LiveQueryEvent.Value -> event.value
            is LiveQueryEvent.Error -> throw event.error
        }

    suspend fun event(key: Key): LiveQueryEvent<Value> {
        val entry = acquire(key)
        try {
            while (true) {
                val awaited =
                    synchronized(lock) {
                        entry.retired?.let { throw it }
                        entry.latest to entry.changed
                    }
                awaited.first?.let { return it }
                awaited.second.await()
            }
        } finally {
            release(key, entry)
        }
    }

    fun cancelAll(error: BridgeException) {
        val removed =
            synchronized(lock) {
                entries.values.toList().also { values ->
                    entries.clear()
                    values.forEach { entry ->
                        entry.retired = error
                        entry.changed.complete(Unit)
                    }
                }
            }
        removed.forEach { it.job?.cancel() }
    }

    private fun acquire(key: Key): Entry<Value> {
        var created = false
        val entry =
            synchronized(lock) {
                (
                    entries[key] ?: Entry<Value>().also {
                        entries[key] = it
                        created = true
                    }
                ).also { it.waiters++ }
            }
        if (created) start(key, entry)
        trim()
        return entry
    }

    private fun start(
        key: Key,
        entry: Entry<Value>,
    ) {
        var initial = true
        val job =
            flow(key)
                .onEach { event ->
                    val accepted =
                        synchronized(lock) {
                            if (entries[key] !== entry || entry.retired != null) {
                                false
                            } else {
                                entry.latest = event
                                entry.changed.complete(Unit)
                                entry.changed = CompletableDeferred()
                                true
                            }
                        }
                    if (accepted) {
                        onEvent(key, event, initial)
                        initial = false
                    }
                }.launchIn(scope)
        synchronized(lock) {
            if (entries[key] === entry) entry.job = job else job.cancel()
        }
    }

    private fun release(
        key: Key,
        entry: Entry<Value>,
    ) {
        synchronized(lock) {
            if (entries[key] === entry) entry.waiters--
        }
        trim()
    }

    private fun trim() {
        val removed =
            synchronized(lock) {
                buildList {
                    while (entries.size > maximumCount) {
                        val candidate = entries.entries.firstOrNull { it.value.waiters == 0 } ?: break
                        entries.remove(candidate.key)?.let(::add)
                    }
                }
            }
        removed.forEach { it.job?.cancel() }
    }
}

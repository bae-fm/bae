package fm.bae.app.playback

import fm.bae.app.data.LiveQueryEvent
import java.util.LinkedHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import uniffi.bae_bridge.BridgeException

private class LiveProjection<T>(
    private val scope: CoroutineScope,
    private val flow: Flow<LiveQueryEvent<T>>,
    private val onEvent: (LiveQueryEvent<T>, isUpdate: Boolean) -> Unit,
) {
    private val lock = Any()
    private var eventDelivered = false
    private var started = false
    private var cancelled = false
    private var job: Job? = null

    fun start() {
        val shouldStart =
            synchronized(lock) {
                if (started || cancelled) {
                    false
                } else {
                    started = true
                    true
                }
            }
        if (!shouldStart) return
        val launched =
            flow
                .onEach { event ->
                    val isUpdate =
                        synchronized(lock) {
                            val update = eventDelivered
                            eventDelivered = true
                            update
                        }
                    onEvent(event, isUpdate)
                }.launchIn(scope)
        synchronized(lock) {
            if (cancelled) launched.cancel() else job = launched
        }
    }

    fun cancel() {
        val activeJob =
            synchronized(lock) {
                cancelled = true
                job
            }
        activeJob?.cancel()
    }
}

internal class LiveProjectionCache<Key, Value>(
    private val scope: CoroutineScope,
    private val maximumCount: Int,
    private val flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    private val isRetained: (Key) -> Boolean,
    private val onChanged: (Key, Value) -> Unit = { _, _ -> },
    private val onError: (Key, previousValue: Value?, BridgeException) -> Unit,
    private val onEntryCreated: (Key) -> Unit = {},
    private val onEntryRemoved: (Key) -> Unit = {},
) {
    private class Entry<Value>(
        val identity: Any,
        val projection: LiveProjection<Value>,
        var waiters: Int = 0,
        var cancelWhenUnused: Boolean = false,
        var delivered: Delivered<Value>? = null,
        var latest: LiveQueryEvent<Value>? = null,
        var changed: CompletableDeferred<Unit> = CompletableDeferred(),
    )

    private data class Delivered<Value>(val value: Value)

    private data class Changed<Value>(val value: Value)

    private val lock = Any()
    private val projections = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun value(key: Key): Value {
        val entry = acquire(key)
        return try {
            awaitValue(key, entry)
        } finally {
            release(key, entry.identity)
        }
    }

    fun ensure(key: Key) {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
                        onEntryCreated(key)
                        created = true
                    }).also { it.cancelWhenUnused = false }
            }
        trimAndCancel()
        if (created) entry.projection.start()
    }

    fun cancelWhenUnused(key: Key) {
        val cancelled =
            synchronized(lock) {
                projections[key]?.let { entry ->
                    if (isRetained(key)) {
                        entry.cancelWhenUnused = false
                        null
                    } else if (entry.waiters == 0) {
                        projections.remove(key)
                    } else {
                        entry.cancelWhenUnused = true
                        null
                    }
                }
            }
        cancelled?.let { remove(key, it) }
    }

    fun cancelAll() {
        val cancelled =
            synchronized(lock) {
                projections.toList().also { projections.clear() }
            }
        cancelled.forEach { (key, entry) -> remove(key, entry) }
    }

    private fun acquire(key: Key): Entry<Value> {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
                        onEntryCreated(key)
                        created = true
                    }).also { it.waiters++ }
            }
        trimAndCancel()
        if (created) entry.projection.start()
        return entry
    }

    private fun createEntry(key: Key): Entry<Value> {
        val identity = Any()
        val projection =
            LiveProjection(scope, flow(key)) { event, isUpdate ->
                apply(key, identity, event, isUpdate)
            }
        return Entry(identity, projection)
    }

    private fun release(
        key: Key,
        identity: Any,
    ) {
        val cancelled =
            synchronized(lock) {
                projections[key]?.takeIf { it.identity === identity }?.let { entry ->
                    check(entry.waiters > 0)
                    entry.waiters--
                    if (entry.waiters == 0 && entry.cancelWhenUnused && !isRetained(key)) {
                        projections.remove(key)
                    } else {
                        null
                    }
                }
            }
        cancelled?.let { remove(key, it) }
        trimAndCancel()
    }

    private suspend fun awaitValue(
        key: Key,
        entry: Entry<Value>,
    ): Value {
        while (true) {
            val (event, changed) =
                synchronized(lock) {
                    val current = projections[key]?.takeIf { it.identity === entry.identity }
                        ?: throw CancellationException("live projection removed")
                    current.latest to current.changed
                }
            if (event != null) {
                return when (event) {
                    is LiveQueryEvent.Value -> event.value
                    is LiveQueryEvent.Error -> throw event.error
                }
            }
            changed.await()
        }
    }

    private fun apply(
        key: Key,
        identity: Any,
        event: LiveQueryEvent<Value>,
        isUpdate: Boolean,
    ) {
        var changedSignal: CompletableDeferred<Unit>? = null
        var changedValue: Changed<Value>? = null
        var error: Pair<Value?, BridgeException>? = null
        var callbackPinned = false
        synchronized(lock) {
            val entry = projections[key]?.takeIf { it.identity === identity } ?: return
            when (event) {
                is LiveQueryEvent.Value -> {
                    entry.delivered = Delivered(event.value)
                    if (isUpdate) {
                        changedValue = Changed(event.value)
                    }
                }
                is LiveQueryEvent.Error -> error = entry.delivered?.value to event.error
            }
            entry.latest = event
            changedSignal = entry.changed
            entry.changed = CompletableDeferred()
            callbackPinned = changedValue != null || error != null
            if (callbackPinned) entry.waiters++
        }
        changedSignal?.complete(Unit)
        try {
            changedValue?.let { onChanged(key, it.value) }
            error?.let { (previous, exception) -> onError(key, previous, exception) }
        } finally {
            if (callbackPinned) release(key, identity)
        }
    }

    private fun trimAndCancel() {
        val evicted =
            synchronized(lock) {
                buildList {
                    while (projections.size > maximumCount) {
                        val candidate =
                            projections.entries.firstOrNull { (key, entry) ->
                                entry.waiters == 0 && !isRetained(key)
                            } ?: break
                        projections.remove(candidate.key)?.let { add(candidate.key to it) }
                    }
                }
            }
        evicted.forEach { (key, entry) -> remove(key, entry) }
    }

    private fun remove(
        key: Key,
        entry: Entry<Value>,
    ) {
        entry.changed.cancel()
        entry.projection.cancel()
        onEntryRemoved(key)
    }
}

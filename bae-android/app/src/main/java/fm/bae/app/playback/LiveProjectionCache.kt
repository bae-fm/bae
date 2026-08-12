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
    private val onError: (Key, previousValue: Value?, BridgeException, isUpdate: Boolean) -> Unit,
) {
    private class Entry<Value>(
        val identity: Any,
        val projection: LiveProjection<Value>,
        var waiters: Int = 0,
        var cancelWhenUnused: Boolean = false,
        var delivered: Delivered<Value>? = null,
        var latest: LiveQueryEvent<Value>? = null,
        var changed: CompletableDeferred<Unit> = CompletableDeferred(),
        var retiredError: BridgeException? = null,
    )

    private data class Awaited<Value>(
        val event: LiveQueryEvent<Value>?,
        val changed: CompletableDeferred<Unit>,
    )

    private data class Delivered<Value>(val value: Value)

    private data class Changed<Value>(val value: Value)

    private val lock = Any()
    private val projections = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun event(key: Key): LiveQueryEvent<Value> {
        val entry = acquire(key)
        return try {
            awaitEvent(entry)
        } finally {
            release(key, entry.identity)
        }
    }

    suspend fun value(key: Key): Value =
        when (val event = event(key)) {
            is LiveQueryEvent.Value -> event.value
            is LiveQueryEvent.Error -> throw event.error
        }

    fun ensure(key: Key) {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
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
        cancelled?.let(::cancel)
    }

    fun cancelAll() {
        val cancelled =
            synchronized(lock) {
                projections.values.toList().also { projections.clear() }
            }
        cancelled.forEach(::cancel)
    }

    fun retireWhere(
        predicate: (Key) -> Boolean,
        error: BridgeException,
    ) {
        val retired =
            synchronized(lock) {
                projections.entries
                    .filter { predicate(it.key) }
                    .map { (key, entry) ->
                        val signal = entry.changed
                        entry.retiredError = error
                        entry.changed = CompletableDeferred()
                        projections.remove(key)
                        entry to signal
                    }
            }
        retired.forEach { (entry, signal) ->
            signal.complete(Unit)
            cancel(entry)
        }
    }

    private fun acquire(key: Key): Entry<Value> {
        var created = false
        val entry =
            synchronized(lock) {
                (projections[key]
                    ?: createEntry(key).also {
                        projections[key] = it
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
                val entry = projections[key]?.takeIf { it.identity === identity }
                if (entry != null) {
                    check(entry.waiters > 0)
                    entry.waiters--
                    if (entry.waiters == 0 && entry.cancelWhenUnused && !isRetained(key)) {
                        projections.remove(key)
                    } else {
                        null
                    }
                } else {
                    null
                }
            }
        cancelled?.let(::cancel)
        trimAndCancel()
    }

    private suspend fun awaitEvent(entry: Entry<Value>): LiveQueryEvent<Value> {
        while (true) {
            val awaited =
                synchronized(lock) {
                    entry.retiredError?.let { throw it }
                    Awaited(entry.latest, entry.changed)
                }
            if (awaited.event != null) {
                return checkNotNull(awaited.event)
            }
            awaited.changed.await()
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
            if (entry.retiredError != null) return
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
            error?.let { (previous, exception) -> onError(key, previous, exception, isUpdate) }
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
                        projections.remove(candidate.key)?.let(::add)
                    }
                }
            }
        evicted.forEach(::cancel)
    }

    private fun cancel(entry: Entry<Value>) {
        entry.projection.cancel()
    }
}

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
import java.util.concurrent.atomic.AtomicLong

internal class AcceptedEventSequence {
    private val next = AtomicLong()

    fun next(): Long = next.incrementAndGet()
}

internal data class AcceptedValue<Value>(
    val value: Value,
    val sequence: Long,
    val identity: Any,
)

internal data class AcceptedEvent<Value>(
    val event: LiveQueryEvent<Value>,
    val isInitial: Boolean,
    val sequence: Long,
    val identity: Any,
)

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
    private val acceptedEventSequence: AcceptedEventSequence = AcceptedEventSequence(),
    private val onAcceptedRead: ((Key, AcceptedValue<Value>) -> Unit)? = null,
    private val onAcceptedChange: (Key, AcceptedEvent<Value>) -> Unit,
) {
    private class Entry<Value>(
        val identity: Any,
        val projection: LiveProjection<Value>,
        var waiters: Int = 0,
        var cancelWhenUnused: Boolean = false,
        var latest: LiveQueryEvent<Value>? = null,
        var changed: CompletableDeferred<Unit> = CompletableDeferred(),
        var retiredError: BridgeException? = null,
    )

    private data class Awaited<Value>(
        val event: LiveQueryEvent<Value>?,
        val changed: CompletableDeferred<Unit>,
    )

    private val lock = Any()
    private val projections = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun event(key: Key): LiveQueryEvent<Value> {
        val entry = acquire(key)
        return try {
            awaitAcceptedEvent(key, entry)
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
        prepareRetireWhere(predicate, error).invoke()
    }

    fun prepareRetireWhere(
        predicate: (Key) -> Boolean,
        error: BridgeException,
    ): () -> Unit {
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
        return {
            retired.forEach { (entry, signal) ->
                signal.complete(Unit)
                cancel(entry)
            }
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

    private suspend fun awaitAcceptedEvent(
        key: Key,
        entry: Entry<Value>,
    ): LiveQueryEvent<Value> {
        while (true) {
            val event = awaitEvent(entry)
            if (acceptCurrentRead(key, entry, event)) return event
        }
    }

    private fun acceptCurrentRead(
        key: Key,
        entry: Entry<Value>,
        event: LiveQueryEvent<Value>,
    ): Boolean {
        var acceptedValue: AcceptedValue<Value>? = null
        val accepted =
            synchronized(lock) {
                entry.retiredError?.let { throw it }
                val current = projections[key]?.takeIf { it.identity === entry.identity }
                checkNotNull(current) { "live projection disappeared while it had an active waiter" }
                if (entry.latest !== event) {
                    false
                } else {
                    if (event is LiveQueryEvent.Value && onAcceptedRead != null) {
                        acceptedValue =
                            AcceptedValue(
                                value = event.value,
                                sequence = acceptedEventSequence.next(),
                                identity = entry.identity,
                            )
                    }
                    true
                }
            }
        acceptedValue?.let { value -> onAcceptedRead?.invoke(key, value) }
        return accepted
    }

    private fun apply(
        key: Key,
        identity: Any,
        event: LiveQueryEvent<Value>,
        isUpdate: Boolean,
    ) {
        var changedSignal: CompletableDeferred<Unit>? = null
        var acceptedEvent: AcceptedEvent<Value>? = null
        synchronized(lock) {
            val entry = projections[key]?.takeIf { it.identity === identity } ?: return
            if (entry.retiredError != null) return
            acceptedEvent =
                AcceptedEvent(
                    event = event,
                    isInitial = !isUpdate,
                    sequence = acceptedEventSequence.next(),
                    identity = entry.identity,
                )
            entry.latest = event
            changedSignal = entry.changed
            entry.changed = CompletableDeferred()
            entry.waiters++
        }
        try {
            acceptedEvent?.let { accepted -> onAcceptedChange(key, accepted) }
        } finally {
            changedSignal?.complete(Unit)
            release(key, identity)
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

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

internal class QueryRevisions<Key> {
    private data class State(
        var references: Int,
        var revision: Long,
    )

    private val lock = Any()
    private val states = mutableMapOf<Key, State>()

    fun retain(key: Key) {
        synchronized(lock) {
            states[key]?.let { it.references++ } ?: run { states[key] = State(references = 1, revision = 0) }
        }
    }

    fun release(key: Key) {
        synchronized(lock) {
            val state = checkNotNull(states[key])
            check(state.references > 0)
            state.references--
            if (state.references == 0) states.remove(key)
        }
    }

    fun current(key: Key): Long = synchronized(lock) { states[key]?.revision ?: 0 }

    fun pageEvent(
        key: Key,
        previousRevision: Long?,
        isUpdate: Boolean,
    ): Long =
        synchronized(lock) {
            val current = checkNotNull(states[key]).revision
            when {
                !isUpdate || previousRevision == null -> current
                current > previousRevision -> current
                else -> Math.incrementExact(current)
            }
        }

    fun observerEvent(
        key: Key,
        event: LiveQueryEvent<*>,
        isUpdate: Boolean,
    ): Long =
        synchronized(lock) {
            val state = checkNotNull(states[key])
            if (isUpdate && event is LiveQueryEvent.Value) {
                state.revision = Math.incrementExact(state.revision)
            }
            state.revision
        }

    fun advanceTo(
        key: Key,
        revision: Long,
    ) {
        synchronized(lock) {
            val state = checkNotNull(states[key])
            if (revision > state.revision) state.revision = revision
        }
    }
}

internal data class ProjectionChange<Value>(
    val previousValue: Value?,
    val currentValue: Value,
    val previousRevision: Long,
    val revision: Long,
    val recoveredFromError: Boolean,
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
    private val minimumRevision: (Key) -> Long = { 0 },
    private val eventRevision: (Key, LiveQueryEvent<Value>, previousRevision: Long?, isUpdate: Boolean) -> Long =
        { _, _, previous, _ -> previous ?: 0 },
    private val onChanged: (Key, ProjectionChange<Value>) -> Unit = { _, _ -> },
    private val onError: (Key, BridgeException) -> Unit,
    private val onEntryCreated: (Key) -> Unit = {},
    private val onEntryRemoved: (Key) -> Unit = {},
) {
    private class Entry<Value>(
        val identity: Any,
        val projection: LiveProjection<Value>,
        var waiters: Int = 0,
        var cancelWhenUnused: Boolean = false,
        var delivered: Delivered<Value>? = null,
        var latest: RevisionEvent<Value>? = null,
        var changed: CompletableDeferred<Unit> = CompletableDeferred(),
    )

    private data class Delivered<Value>(val value: Value)

    private data class RevisionEvent<Value>(
        val revision: Long,
        val event: LiveQueryEvent<Value>,
    )

    private val lock = Any()
    private val projections = LinkedHashMap<Key, Entry<Value>>(16, 0.75f, true)

    suspend fun value(key: Key): Value {
        val entry = acquire(key)
        return try {
            valueAtLeast(key, entry, minimumRevision(key))
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
        cancelled?.let { entry ->
            entry.changed.cancel()
            entry.projection.cancel()
            onEntryRemoved(key)
        }
    }

    fun cancelAll() {
        val cancelled =
            synchronized(lock) {
                projections.toList().also { projections.clear() }
            }
        cancelled.forEach { (key, entry) ->
            entry.changed.cancel()
            entry.projection.cancel()
            onEntryRemoved(key)
        }
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
        cancelled?.let { entry ->
            entry.changed.cancel()
            entry.projection.cancel()
            onEntryRemoved(key)
        }
        trimAndCancel()
    }

    private suspend fun valueAtLeast(
        key: Key,
        entry: Entry<Value>,
        minimumRevision: Long,
    ): Value {
        while (true) {
            val (event, changed) =
                synchronized(lock) {
                    val current = projections[key]?.takeIf { it.identity === entry.identity }
                        ?: throw CancellationException("live projection removed")
                    val delivered = current.latest?.takeIf { it.revision >= minimumRevision }?.event
                    delivered to current.changed
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
        var change: ProjectionChange<Value>? = null
        var error: BridgeException? = null
        var callbackPinned = false
        synchronized(lock) {
            val entry = projections[key]?.takeIf { it.identity === identity } ?: return
            val previousEvent = entry.latest
            val revision = eventRevision(key, event, previousEvent?.revision, isUpdate)
            when (event) {
                is LiveQueryEvent.Value -> {
                    val previous = entry.delivered?.value
                    entry.delivered = Delivered(event.value)
                    if (isUpdate) {
                        change =
                            ProjectionChange(
                                previousValue = previous,
                                currentValue = event.value,
                                previousRevision = checkNotNull(previousEvent).revision,
                                revision = revision,
                                recoveredFromError = previousEvent?.event is LiveQueryEvent.Error,
                            )
                    }
                }
                is LiveQueryEvent.Error -> error = event.error
            }
            entry.latest = RevisionEvent(revision, event)
            changedSignal = entry.changed
            entry.changed = CompletableDeferred()
            callbackPinned = change != null || error != null
            if (callbackPinned) entry.waiters++
        }
        changedSignal?.complete(Unit)
        try {
            change?.let { onChanged(key, it) }
            error?.let { onError(key, it) }
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
        evicted.forEach { (key, entry) ->
            entry.changed.cancel()
            entry.projection.cancel()
            onEntryRemoved(key)
        }
    }
}

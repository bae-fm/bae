package fm.bae.app.data

import android.os.Handler
import android.os.Looper
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap

/**
 * The latest value of each key a fast keyed stream names, such as how far each
 * downloading release has got. The stream's callbacks write here on the thread
 * they arrive on, with no hop to the main thread; the View drawing a key reads
 * its latest value once per display frame while the key is active, so the
 * screen takes values at its own refresh rate however fast they come. Only a
 * key starting or stopping reaches the main thread, to start or stop the Views
 * drawing it. Nothing here is Compose state.
 */
class LatestByKey<V : Any> {
    private val values = ConcurrentHashMap<String, V>()
    private val watchers = ConcurrentHashMap<String, MutableSet<() -> Unit>>()
    private val main by lazy { Handler(Looper.getMainLooper()) }

    /** The key's latest value; null while the stream names none for it. */
    fun latest(key: String): V? = values[key]

    /** The stream named [key] with [value]. */
    fun set(
        key: String,
        value: V,
    ) {
        if (values.put(key, value) == null) tell(key)
    }

    /** The stream stopped naming [key]. */
    fun remove(key: String) {
        if (values.remove(key) != null) tell(key)
    }

    /** The stream named every key it holds: [all], and none other. */
    fun reset(all: Map<String, V>) {
        val gone = values.keys.filter { it !in all }
        gone.forEach(::remove)
        all.forEach { (key, value) -> set(key, value) }
    }

    /** Run [changed] on the main thread each time [key] starts or stops being named, until [stopWatching]. */
    fun watch(
        key: String,
        changed: () -> Unit,
    ) {
        watchers.getOrPut(key) { Collections.newSetFromMap(ConcurrentHashMap()) }.add(changed)
    }

    fun stopWatching(
        key: String,
        changed: () -> Unit,
    ) {
        watchers[key]?.remove(changed)
    }

    /** Post once to the main thread for a key that started or stopped, for the Views watching it. */
    private fun tell(key: String) {
        val watching = watchers[key]?.toList().orEmpty()
        if (watching.isEmpty()) return
        main.post { watching.forEach { it() } }
    }
}

/** One key of a fast keyed stream, which a composable hands to the View that draws it. */
data class LatestSource<V : Any>(
    val store: LatestByKey<V>,
    val key: String,
) {
    val latest: V? get() = store.latest(key)
}

/**
 * The latest value of a fast stream with one value, such as the files being
 * sent together: a one-key [LatestByKey], holding the value while it is
 * active and none while it isn't.
 */
class LatestOne<V : Any> {
    private val store = LatestByKey<V>()

    /** The stream's value now, active. */
    fun set(value: V) = store.set(KEY, value)

    /** The stream went idle. */
    fun clear() = store.remove(KEY)

    val latest: V? get() = store.latest(KEY)

    /** The stream, for the View that draws it. */
    val source: LatestSource<V> = LatestSource(store, KEY)

    private companion object {
        const val KEY = ""
    }
}

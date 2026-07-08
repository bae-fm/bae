package fm.bae.app.widget

import android.content.Context
import fm.bae.app.BaeLogger
import fm.bae.app.playback.NowPlaying
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

private const val TAG = "bae.WidgetSnapshot"
private val logger = BaeLogger(TAG)

/**
 * The now-playing state the home-screen widget renders. The widget runs in the
 * launcher's process and can't host bae-core, so the app writes this compact
 * projection to a file whenever now-playing changes; the widget reads it back on
 * render. [track] is null when nothing is playing (the widget's empty state).
 */
data class WidgetSnapshot(
    val track: WidgetTrack?,
    val isPlaying: Boolean,
) {
    companion object {
        val EMPTY = WidgetSnapshot(track = null, isPlaying = false)

        /**
         * Project the player's live now-playing state into the widget snapshot.
         * The sole map from playback state to what the widget shows — hooked off
         * the player's existing [NowPlaying]/isPlaying projection, not a second
         * event subscription.
         */
        fun from(
            nowPlaying: NowPlaying?,
            isPlaying: Boolean,
        ): WidgetSnapshot =
            WidgetSnapshot(
                track = nowPlaying?.let { WidgetTrack(it.title, it.artist, it.coverImageId) },
                isPlaying = isPlaying,
            )
    }
}

/**
 * The track fields the widget shows. [coverImageId] is the release id the widget
 * resolves to cover bytes through the same `content://` artwork path the media
 * browse clients use ([fm.bae.app.playback.ArtworkContentProvider]).
 */
data class WidgetTrack(
    val title: String,
    val artist: String,
    val coverImageId: String?,
)

private const val KEY_IS_PLAYING = "isPlaying"
private const val KEY_TRACK = "track"
private const val KEY_TITLE = "title"
private const val KEY_ARTIST = "artist"
private const val KEY_COVER_IMAGE_ID = "coverImageId"

internal fun WidgetSnapshot.toJson(): String {
    val obj = JSONObject()
    obj.put(KEY_IS_PLAYING, isPlaying)
    track?.let {
        val t = JSONObject()
        t.put(KEY_TITLE, it.title)
        t.put(KEY_ARTIST, it.artist)
        it.coverImageId?.let { cover -> t.put(KEY_COVER_IMAGE_ID, cover) }
        obj.put(KEY_TRACK, t)
    }
    return obj.toString()
}

internal fun parseWidgetSnapshot(json: String): WidgetSnapshot {
    val obj = JSONObject(json)
    val track =
        obj.optJSONObject(KEY_TRACK)?.let {
            WidgetTrack(
                title = it.getString(KEY_TITLE),
                artist = it.getString(KEY_ARTIST),
                coverImageId = if (it.has(KEY_COVER_IMAGE_ID)) it.getString(KEY_COVER_IMAGE_ID) else null,
            )
        }
    return WidgetSnapshot(track = track, isPlaying = obj.getBoolean(KEY_IS_PLAYING))
}

private const val SNAPSHOT_FILE_NAME = "now_playing_widget.json"

/**
 * Reads and writes the widget snapshot to a file in app storage. One file is the
 * single source both the writer (the open library's playback observer) and the
 * reader (the widget's render pass) share; a live library open overwrites it,
 * and it survives process death so the widget can render the last state cold.
 */
class WidgetSnapshotStore(
    private val file: File,
) {
    constructor(context: Context) : this(File(context.filesDir, SNAPSHOT_FILE_NAME))

    /** Write [snapshot] atomically (temp file then rename within the same dir)
     *  so a concurrent render never reads a half-written file. */
    suspend fun write(snapshot: WidgetSnapshot) {
        withContext(Dispatchers.IO) {
            val tmp = File(file.parentFile, "$SNAPSHOT_FILE_NAME.tmp")
            tmp.writeText(snapshot.toJson())
            if (!tmp.renameTo(file)) {
                tmp.delete()
                logger.error("could not replace widget snapshot at ${file.path}")
            }
        }
    }

    /** The last written snapshot, or [WidgetSnapshot.EMPTY] when none has been
     *  written yet (first run) or the file can't be parsed. */
    suspend fun read(): WidgetSnapshot =
        withContext(Dispatchers.IO) {
            if (!file.exists()) {
                logger.debug("no widget snapshot yet; empty state")
                return@withContext WidgetSnapshot.EMPTY
            }
            try {
                parseWidgetSnapshot(file.readText())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.warning("could not read widget snapshot; empty state", e)
                WidgetSnapshot.EMPTY
            }
        }
}

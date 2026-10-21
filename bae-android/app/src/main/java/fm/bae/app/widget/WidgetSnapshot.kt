package fm.bae.app.widget

import android.content.Context
import fm.bae.app.BaeLogger
import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.NowPlaying
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType
import java.io.File

private const val TAG = "bae.WidgetSnapshot"
private val logger = BaeLogger(TAG)

/**
 * The last track this device played, as the home-screen widget shows it. A
 * cold render (the widget drawn by a process with no library open) can't ask
 * bae-core, so the open library writes this compact projection to a file
 * whenever now-playing changes and the widget reads it back on render. [track]
 * is null when nothing is playing (the widget's empty state).
 *
 * Whether the track is playing is not part of it: that is a fact about this
 * process's open library, true only while it is alive and playing, which a file
 * left behind by a process that died can't speak for ([widgetContent]).
 */
data class WidgetSnapshot(
    val track: WidgetTrack?,
) {
    companion object {
        val EMPTY = WidgetSnapshot(track = null)

        /**
         * Project the player's live now-playing state into the widget snapshot.
         * The sole map from playback state to what the widget shows — hooked off
         * the player's existing [NowPlaying] projection, not a second event
         * subscription.
         */
        fun from(nowPlaying: NowPlaying?): WidgetSnapshot =
            WidgetSnapshot(track = nowPlaying?.let { WidgetTrack(it.title, it.artist, it.coverImage) })
    }
}

/** What the widget draws: [track], with a pause button only while [isPlaying]. */
internal data class WidgetContent(
    val track: WidgetTrack?,
    val isPlaying: Boolean,
)

/**
 * The widget's content from the saved [snapshot] and the open library's
 * [livePlayer], null when this process has no library open. The track shows as
 * playing only while that player is playing it; a snapshot alone, read by a
 * process that just started or after the library closed, shows the play button.
 */
internal fun widgetContent(
    snapshot: WidgetSnapshot,
    livePlayer: BaeCorePlayer?,
): WidgetContent = WidgetContent(track = snapshot.track, isPlaying = livePlayer?.isPlaying?.value == true)

/**
 * The track fields the widget shows. [coverImage] is the reference the widget
 * resolves to cover bytes through the same `content://` artwork path the media
 * browse clients use ([fm.bae.app.playback.ArtworkContentProvider]).
 */
data class WidgetTrack(
    val title: String,
    val artist: String,
    val coverImage: BridgeImageRef?,
)

private const val KEY_TRACK = "track"
private const val KEY_TITLE = "title"
private const val KEY_ARTIST = "artist"
private const val KEY_COVER = "cover"
private const val KEY_COVER_TYPE = "type"
private const val KEY_COVER_ID = "id"
private const val KEY_COVER_VERSION = "version"

internal fun WidgetSnapshot.toJson(): String {
    val obj = JSONObject()
    track?.let {
        val t = JSONObject()
        t.put(KEY_TITLE, it.title)
        t.put(KEY_ARTIST, it.artist)
        it.coverImage?.let { cover ->
            t.put(
                KEY_COVER,
                JSONObject()
                    .put(KEY_COVER_TYPE, cover.imageType.name)
                    .put(KEY_COVER_ID, cover.id)
                    .put(KEY_COVER_VERSION, cover.version),
            )
        }
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
                coverImage = it.optJSONObject(KEY_COVER)?.let(::parseCover),
            )
        }
    return WidgetSnapshot(track = track)
}

/** The image reference a snapshot's cover object names, or null when its kind
 *  isn't one the bridge declares (a snapshot written by an older build). */
private fun parseCover(cover: JSONObject): BridgeImageRef? {
    val name = cover.getString(KEY_COVER_TYPE)
    val imageType = BridgeLibraryImageType.entries.firstOrNull { it.name == name }
    if (imageType == null) {
        logger.warning("widget snapshot names an unknown image kind $name; rendering without art")
        return null
    }
    return BridgeImageRef(
        id = cover.getString(KEY_COVER_ID),
        version = cover.getString(KEY_COVER_VERSION),
        imageType = imageType,
    )
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

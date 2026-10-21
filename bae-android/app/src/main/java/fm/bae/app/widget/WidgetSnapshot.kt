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

/** What the widget draws. */
internal sealed interface WidgetContent {
    /** [track], with a pause button only while [isPlaying]. */
    data class Track(
        val track: WidgetTrack,
        val isPlaying: Boolean,
    ) : WidgetContent

    /** Nothing has played, or the saved file was from an older version and is gone. */
    data object NothingPlaying : WidgetContent

    /** The saved file is damaged, which the app's one-step write rules out: a bug, reported. */
    data object SomethingWentWrong : WidgetContent
}

/**
 * The widget's content from what was [read] and the open library's
 * [livePlayer], null when this process has no library open. The track shows as
 * playing only while that player is playing it; a saved track alone, read by a
 * process that just started or after the library closed, shows the play button.
 */
internal fun widgetContent(
    read: WidgetRead,
    livePlayer: BaeCorePlayer?,
): WidgetContent =
    when (read) {
        is WidgetRead.Saved -> {
            read.snapshot.track?.let { WidgetContent.Track(it, isPlaying = livePlayer?.isPlaying?.value == true) }
                ?: WidgetContent.NothingPlaying
        }

        WidgetRead.NothingSaved -> {
            WidgetContent.NothingPlaying
        }

        is WidgetRead.Damaged -> {
            WidgetContent.SomethingWentWrong
        }
    }

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

private const val KEY_FORMAT = "format"

/**
 * The file format this build writes. A file without it, or with another, was
 * written by an older version: it is deleted rather than read.
 */
private const val CURRENT_FORMAT = 2

private const val KEY_TRACK = "track"
private const val KEY_TITLE = "title"
private const val KEY_ARTIST = "artist"
private const val KEY_COVER = "cover"
private const val KEY_COVER_TYPE = "type"
private const val KEY_COVER_ID = "id"
private const val KEY_COVER_VERSION = "version"

internal fun WidgetSnapshot.toJson(): String {
    val obj = JSONObject()
    obj.put(KEY_FORMAT, CURRENT_FORMAT)
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

/** The snapshot a current-format file holds; throws when the file is damaged. */
private fun parseWidgetSnapshot(obj: JSONObject): WidgetSnapshot {
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

/** The image reference a cover object names; this build wrote it, so its kind is one the bridge declares. */
private fun parseCover(cover: JSONObject): BridgeImageRef =
    BridgeImageRef(
        id = cover.getString(KEY_COVER_ID),
        version = cover.getString(KEY_COVER_VERSION),
        imageType = BridgeLibraryImageType.valueOf(cover.getString(KEY_COVER_TYPE)),
    )

/** What reading the saved widget file found. */
internal sealed interface WidgetRead {
    data class Saved(
        val snapshot: WidgetSnapshot,
    ) : WidgetRead

    /** No file yet, or one from an older version, now deleted. */
    data object NothingSaved : WidgetRead

    /** A current-format file that doesn't parse: [error] is why. */
    data class Damaged(
        val error: Exception,
    ) : WidgetRead
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

    /**
     * The saved snapshot. Only the current format is read: a file from an
     * older version is deleted and logged, and reads as nothing saved. A
     * current-format file is written in one atomic step, so one that doesn't
     * parse is damaged — a bug for the caller to report.
     */
    internal suspend fun read(): WidgetRead =
        withContext(Dispatchers.IO) {
            if (!file.exists()) {
                logger.debug("no widget snapshot yet; nothing saved")
                return@withContext WidgetRead.NothingSaved
            }
            val obj =
                try {
                    JSONObject(file.readText())
                } catch (e: org.json.JSONException) {
                    return@withContext WidgetRead.Damaged(e)
                }
            val format = obj.optInt(KEY_FORMAT, 0)
            if (format != CURRENT_FORMAT) {
                logger.info("deleting a widget snapshot in format $format, from an older version")
                if (!file.delete()) logger.error("could not delete the older widget snapshot at ${file.path}")
                return@withContext WidgetRead.NothingSaved
            }
            try {
                WidgetRead.Saved(parseWidgetSnapshot(obj))
            } catch (e: CancellationException) {
                throw e
            } catch (e: org.json.JSONException) {
                WidgetRead.Damaged(e)
            } catch (e: IllegalArgumentException) {
                WidgetRead.Damaged(e)
            }
        }
}

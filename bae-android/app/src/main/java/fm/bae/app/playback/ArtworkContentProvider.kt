package fm.bae.app.playback

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlin.concurrent.thread

private const val TAG = "bae.ArtworkContentProvider"
private val logger = BaeLogger(TAG)
private const val AUTHORITY_SUFFIX = ".artwork"
private const val COVER_PATH = "cover"

/**
 * Serves release/composer/work cover bytes to media-browse clients (Android
 * Auto, Bluetooth head units) over a `content://` URI. The browse tree tags
 * each item's artwork with a URI ([uriFor]); Media3 grants the connected
 * controller read access to it and the controller fetches the bytes lazily,
 * so cover bytes never ride inline in the browse payload across Binder (where
 * a page of covers would blow the transaction size limit).
 *
 * The bytes come from the same open library the rest of the app reads — the
 * bridge's `fetchCoverImageBytes`, through [fm.bae.app.data.Library.imageBytes]
 * — so there is one cover-loading path, not a parallel one.
 */
class ArtworkContentProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String = "image/*"

    override fun openFile(
        uri: Uri,
        mode: String,
    ): ParcelFileDescriptor? {
        val coverId = uri.lastPathSegment
        if (coverId == null) {
            logger.warning("artwork request with no cover id: $uri")
            return null
        }
        val session = AppSessionHolder.currentSession()
        if (session == null) {
            logger.debug("artwork request $coverId with no open library; no bytes")
            return null
        }
        val bytes =
            try {
                runBlocking { session.library.imageBytes(coverId) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("failed to load artwork bytes for $coverId", e)
                return null
            }
        if (bytes == null) {
            logger.debug("no artwork bytes for cover $coverId")
            return null
        }
        return pipeOf(bytes, coverId)
    }

    /** Stream [bytes] through a pipe so the client reads them without the bytes
     *  crossing the Binder transaction. */
    private fun pipeOf(
        bytes: ByteArray,
        coverId: String,
    ): ParcelFileDescriptor {
        val pipe = ParcelFileDescriptor.createPipe()
        thread(name = "artwork-$coverId") {
            try {
                ParcelFileDescriptor.AutoCloseOutputStream(pipe[1]).use { it.write(bytes) }
            } catch (e: Exception) {
                logger.warning("artwork stream for $coverId interrupted", e)
            }
        }
        return pipe[0]
    }

    // A read-only byte source: the query/mutation surface is unused.
    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun insert(
        uri: Uri,
        values: ContentValues?,
    ): Uri? = null

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    override fun delete(
        uri: Uri,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    companion object {
        /** The `content://` URI a browse client reads [coverId]'s bytes from.
         *  The authority carries the app's own package (distinct across the full
         *  and baeium editions), matching the manifest's `${applicationId}`. */
        fun uriFor(
            context: Context,
            coverId: String,
        ): Uri =
            Uri
                .Builder()
                .scheme("content")
                .authority(context.packageName + AUTHORITY_SUFFIX)
                .appendPath(COVER_PATH)
                .appendPath(coverId)
                .build()
    }
}

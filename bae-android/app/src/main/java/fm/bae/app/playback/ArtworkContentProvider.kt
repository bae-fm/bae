package fm.bae.app.playback

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import fm.bae.app.AppSessionHolder
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import kotlin.concurrent.thread
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType

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
 * bridge's `fetchLibraryImageBytes`, through [fm.bae.app.data.Library.imageBytes]
 * — so there is one cover-loading path, not a parallel one.
 */
class ArtworkContentProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String = "image/*"

    override fun openFile(
        uri: Uri,
        mode: String,
    ): ParcelFileDescriptor? {
        val image = imageFrom(uri)
        val session = AppSessionHolder.currentSession()
        return when {
            image == null -> {
                logger.warning("artwork request with no image reference: $uri")
                null
            }

            session == null -> {
                logger.debug("artwork request ${image.id} with no open library; no bytes")
                null
            }

            else -> {
                pipeFor(image, session)
            }
        }
    }

    /** The pipe a browse client reads [image]'s bytes from, or null when the
     *  open library has no bytes for it. */
    private fun pipeFor(
        image: BridgeImageRef,
        session: OpenLibrary,
    ): ParcelFileDescriptor? {
        val bytes = readCoverBytes(image, session)
        if (bytes == null) {
            logger.debug("no artwork bytes for cover ${image.id}")
            return null
        }
        return pipeOf(bytes, image.id)
    }

    /** Read [image]'s bytes from the open library, or null on failure (a
     *  cancellation propagates so the caller's read is cancelled, not swallowed). */
    private fun readCoverBytes(
        image: BridgeImageRef,
        session: OpenLibrary,
    ): ByteArray? =
        try {
            runBlocking { session.library.imageBytes(image) }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("failed to load artwork bytes for ${image.id}", e)
            null
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
        /** The `content://` URI a browse client reads [image]'s bytes from. The
         *  whole reference rides in the path — kind, subject id, and content
         *  version — so a replaced cover yields a different URI and the client
         *  re-reads instead of showing its cached art. The authority carries the
         *  app's own package (distinct across the full and baeium editions),
         *  matching the manifest's `${applicationId}`. */
        fun uriFor(
            context: Context,
            image: BridgeImageRef,
        ): Uri =
            Uri
                .Builder()
                .scheme("content")
                .authority(context.packageName + AUTHORITY_SUFFIX)
                .appendPath(COVER_PATH)
                .appendPath(image.imageType.name)
                .appendPath(image.id)
                .appendPath(image.version)
                .build()

        /** The image reference a [uriFor] URI names, or null when the path isn't
         *  one this provider minted. */
        private fun imageFrom(uri: Uri): BridgeImageRef? {
            val segments = uri.pathSegments
            if (segments.size != 4 || segments[0] != COVER_PATH) {
                return null
            }
            val imageType =
                BridgeLibraryImageType.entries.firstOrNull { it.name == segments[1] }
                    ?: return null
            return BridgeImageRef(id = segments[2], version = segments[3], imageType = imageType)
        }
    }
}

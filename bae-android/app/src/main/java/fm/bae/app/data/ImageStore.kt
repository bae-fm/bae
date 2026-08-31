package fm.bae.app.data

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.util.LruCache
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.exifinterface.media.ExifInterface
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import java.io.ByteArrayInputStream

private const val TAG = "bae.ImageStore"
private val logger = BaeLogger(TAG)

/**
 * What an image slot shows, and therefore where its bytes come from. A view names
 * the content and renders whatever [ImageStore] hands back.
 */
sealed interface ImageContent {
    /** A curated library image — a release cover or an artist portrait — read by
     *  its versioned reference. */
    data class LibraryImage(
        val image: BridgeImageRef,
    ) : ImageContent

    /** One slot of a release's image strip: its cover, or one of the release's own
     *  image files. Core dispatches the read on the [BridgeGallerySource], so the
     *  UI never picks the byte source itself. */
    data class ReleaseImage(
        val releaseId: String,
        val source: BridgeGallerySource,
    ) : ImageContent

    /** Human-readable description for failure logs. */
    val description: String
        get() =
            when (this) {
                is LibraryImage -> "library image: ${image.imageType} ${image.id}"
                is ReleaseImage -> "release image: $releaseId"
            }
}

/**
 * How much of the source a decode reads.
 *
 * [FitTo] takes the downsampled path, decoding the smallest power-of-two
 * reduction whose longer edge still covers the slot. [Native] decodes at the
 * source's own resolution — what the zoomable viewer asks for once the user
 * pinches in, so a large scan never decodes whole just to fill a screen.
 */
sealed interface DecodeSize {
    data class FitTo(
        val pixels: Int,
    ) : DecodeSize

    data object Native : DecodeSize

    /** The part of a cache key that says at what resolution the entry was
     *  decoded, so one slot's decode never serves another's. */
    val keySuffix: String
        get() =
            when (this) {
                is FitTo -> pixels.toString()
                Native -> "native"
            }
}

/** The decoded cache's independent budgets, one per content kind. */
enum class ImageBucket {
    LIBRARY_IMAGE,
    RELEASE_IMAGE,
}

/** Which cache the decodes of this content live in. */
private val ImageContent.bucket: ImageBucket
    get() =
        when (this) {
            is ImageContent.LibraryImage -> ImageBucket.LIBRARY_IMAGE
            is ImageContent.ReleaseImage -> ImageBucket.RELEASE_IMAGE
        }

private const val MEGABYTE = 1024 * 1024

private const val DECODED_LIBRARY_IMAGE_BUDGET = 48 * MEGABYTE
private const val DECODED_RELEASE_IMAGE_BUDGET = 16 * MEGABYTE
private const val ENCODED_LIBRARY_IMAGE_BUDGET = 16 * MEGABYTE
private const val ENCODED_RELEASE_IMAGE_BUDGET = 8 * MEGABYTE

/**
 * Per-kind byte budgets for the decoded cache. Eviction never crosses buckets, so
 * a native-size release image cannot evict the album grid's covers.
 *
 */
data class DecodedImageBudgets(
    val libraryImage: Int = DECODED_LIBRARY_IMAGE_BUDGET,
    val releaseImage: Int = DECODED_RELEASE_IMAGE_BUDGET,
)

/**
 * Per-kind byte budgets for the encoded-byte cache that sits in front of the
 * bridge. Android keeps this extra layer because an FFI crossing (plus, for a
 * release image, a cloud download and decrypt) dominates the cost of re-showing
 * an image the app has already read.
 *
 * Both kinds are held because their tokens carry
 * content identity — a version that moves with the bytes, an immutable file id —
 * so a held entry can never come to name different bytes.
 */
data class ImageByteBudgets(
    val libraryImage: Int = ENCODED_LIBRARY_IMAGE_BUDGET,
    val releaseImage: Int = ENCODED_RELEASE_IMAGE_BUDGET,
)

/**
 * The app's image pipeline: bytes → decode at the slot's pixel size → bounded
 * decoded cache → synchronous first-frame read. One instance per open library,
 * read from the composition through [LocalImageStore]; views hold no fetch,
 * cache, or decode logic of their own.
 *
 * What a cached decode is pinned to — its token — is the content's identity, so
 * no entry can outlive the bytes it came from: a curated image keys on its
 * `_updated_at` version, and a release file on its file id (immutable — an import
 * mints a fresh id per file, and a re-import mints new ones rather than
 * repointing an existing row).
 */
class ImageStore(
    /** Bytes of a curated library image, or null when no such image exists. */
    private val fetchLibraryImageBytes: suspend (image: BridgeImageRef) -> ByteArray? = { null },
    /** Bytes of one of a release's image-strip slots, downloaded from the
     *  release's cloud home (and decrypted) when it isn't on disk here. */
    private val fetchReleaseImageBytes:
        suspend (releaseId: String, source: BridgeGallerySource) -> ByteArray =
        { _, _ -> throw ImageStoreUnavailable() },
    decodedBudgets: DecodedImageBudgets = DecodedImageBudgets(),
    byteBudgets: ImageByteBudgets = ImageByteBudgets(),
    /** Where fetches and decodes run. Injected so tests drive them on their own
     *  scheduler. */
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    constructor(handle: AppHandle) : this(
        fetchLibraryImageBytes = { handle.fetchLibraryImageBytes(it) },
        fetchReleaseImageBytes = { releaseId, source -> handle.fetchReleaseImageBytes(releaseId, source) },
    )

    private val decoded = DecodedCaches(decodedBudgets)
    private val bytes = ByteCaches(byteBudgets)

    /**
     * The already-decoded bitmap for [content] at this size, or null when it isn't
     * cached. Synchronous, so a view can draw the real art on its very first frame
     * after (re)mounting instead of flashing a placeholder while an async load
     * lands a frame later.
     *
     * This is a memory-only lookup.
     */
    fun cachedImage(
        content: ImageContent,
        size: DecodeSize,
    ): Bitmap? {
        val key = cacheKey(content, size)
        return decoded[content.bucket].get(key)
    }

    /**
     * Decoded bitmap for [content] at [size]: the cached decode when there is one,
     * otherwise fetch the bytes, decode off the main thread at that size, and
     * store the result. Returns null when no such image exists (a library image
     * with no bytes); a fetch or decode failure throws rather than resolving to a
     * blank slot.
     */
    suspend fun image(
        content: ImageContent,
        size: DecodeSize,
    ): Bitmap? {
        val key = cacheKey(content, size)
        val cached = decoded[content.bucket].get(key)
        if (cached != null) return cached
        val bitmap =
            withContext(dispatcher) {
                imageBytes(content)?.let { decodeBytes(it, size) }
            }
        if (bitmap != null) {
            decoded[content.bucket].put(key, bitmap)
        }
        return bitmap
    }

    /**
     * The encoded bytes [content] decodes from: served from the byte cache when
     * they're still held, otherwise read across the bridge and cached under the
     * content's token. Null when no such image exists.
     *
     * Consumers that hand encoded bytes to something else — the artwork
     * `ContentProvider` streaming to a media-browse client — read them here rather
     * than opening a second path to core.
     */
    suspend fun imageBytes(content: ImageContent): ByteArray? = heldOrFetchedBytes(content)

    /** The bytes held for [content] in the byte cache, else the ones core hands
     *  back — held under the content's token when there is one. */
    private suspend fun heldOrFetchedBytes(content: ImageContent): ByteArray? {
        val token = token(content)
        val cache = bytes[content.bucket]
        val held = cache.get(token)
        if (held != null) return held
        val fetched = withContext(dispatcher) { fetchBytes(content) }
        if (fetched != null) {
            cache.put(token, fetched)
        }
        return fetched
    }

    /** Read [content]'s bytes from core. Null when core reports no such image. */
    private suspend fun fetchBytes(content: ImageContent): ByteArray? =
        when (content) {
            is ImageContent.LibraryImage -> {
                fetchLibraryImageBytes(content.image).also {
                    if (it == null) {
                        // Core handed us an image reference but its blob read
                        // returned nothing — note it rather than blanking silently.
                        logger.warning("no bytes for ${content.description}")
                    }
                }
            }

            is ImageContent.ReleaseImage -> {
                fetchReleaseImageBytes(content.releaseId, content.source)
            }
        }

    /**
     * Cache key for a decoded bitmap: its content identity plus the decode
     * resolution, so the now-playing bar's 48dp decode never serves the album
     * header's 140dp slot, and vice versa.
     */
    private fun cacheKey(
        content: ImageContent,
        size: DecodeSize,
    ): String = "${token(content)}#${size.keySuffix}"

    /** What pins a cached entry to the exact bytes it came from. */
    private fun token(content: ImageContent): String =
        when (content) {
            is ImageContent.LibraryImage -> {
                content.image.libraryToken
            }

            is ImageContent.ReleaseImage -> {
                when (val source = content.source) {
                    // A cover slot IS the release's curated cover; its version
                    // moves whenever the bytes do.
                    is BridgeGallerySource.Cover -> source.image.libraryToken

                    // A release file's bytes are immutable per id: an import mints
                    // a fresh id per file and a re-import mints new ones, so an id
                    // never comes to name different bytes.
                    is BridgeGallerySource.ReleaseFile -> "file:${source.fileId}"
                }
            }
        }

    companion object {
        /** A store that resolves nothing — for @Preview composables and the
         *  screenshot scenes, which have no open library to read from. Every slot
         *  settles on its placeholder. */
        fun unresolved(): ImageStore = ImageStore()
    }
}

/** What a curated library image's cached entries are pinned to: its kind, its
 *  subject, and the version that moves whenever its bytes do. */
private val BridgeImageRef.libraryToken: String
    get() = "library:${imageType.name}:$id:$version"

/** A capability this [ImageStore] isn't wired for. */
class ImageStoreUnavailable : Exception("this ImageStore has no fetch for that content")

/**
 * The image store for the open library, provided once at the composition root. No
 * default: rendering an image outside that provider is a wiring bug, so it fails
 * loudly rather than handing out a throwaway per-call store.
 */
val LocalImageStore =
    staticCompositionLocalOf<ImageStore> {
        error("LocalImageStore not provided")
    }

/** The per-kind decoded caches, addressed by bucket. Cost is the bitmap's
 *  own allocation, so a budget is a real memory ceiling. */
private class DecodedCaches(
    budgets: DecodedImageBudgets,
) {
    private val caches =
        mapOf(
            ImageBucket.LIBRARY_IMAGE to bitmapCache(budgets.libraryImage),
            ImageBucket.RELEASE_IMAGE to bitmapCache(budgets.releaseImage),
        )

    /** Every bucket is built in init, so a missing one is a broken map, not a
     *  cache miss — [Map.getValue] throws rather than answering null. */
    operator fun get(bucket: ImageBucket): LruCache<String, Bitmap> = caches.getValue(bucket)

    private fun bitmapCache(maxBytes: Int) =
        object : LruCache<String, Bitmap>(maxBytes) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.allocationByteCount
        }
}

/** The per-kind encoded-byte caches. */
private class ByteCaches(
    budgets: ImageByteBudgets,
) {
    private val caches =
        mapOf(
            ImageBucket.LIBRARY_IMAGE to byteCache(budgets.libraryImage),
            ImageBucket.RELEASE_IMAGE to byteCache(budgets.releaseImage),
        )

    operator fun get(bucket: ImageBucket): LruCache<String, ByteArray> = caches.getValue(bucket)

    private fun byteCache(maxBytes: Int) =
        object : LruCache<String, ByteArray>(maxBytes) {
            override fun sizeOf(
                key: String,
                value: ByteArray,
            ): Int = value.size
        }
}

/**
 * Decode [bytes] to a bitmap at [size], with the source's EXIF orientation
 * applied. Returns null when the bytes aren't an image the platform decodes.
 *
 * Runs on the caller's thread — [ImageStore] only calls it off the main one.
 */
internal fun decodeBytes(
    bytes: ByteArray,
    size: DecodeSize,
): Bitmap? =
    decodeSampled(
        size = size,
        decode = { options -> BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) },
        exif = { ExifInterface(ByteArrayInputStream(bytes)) },
    )

/**
 * The two-pass decode both sources take: [decode] with a bounds-only pass to
 * learn the source's dimensions, then again with the downsample options [size]
 * calls for, and finally the orientation [exif] records. Null when the bounds
 * pass reports no image, or the platform declines the second pass.
 *
 * [exif] is read only once there is a bitmap to orient, so an undecodable source
 * is never parsed twice.
 */
private fun decodeSampled(
    size: DecodeSize,
    decode: (BitmapFactory.Options) -> Bitmap?,
    exif: () -> ExifInterface,
): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    decode(bounds)
    val bitmap = decodeOptions(bounds, size)?.let(decode)
    return bitmap?.let { oriented(it, exif()) }
}

/**
 * Decode options that downsample [bounds] toward [size], or null when the bounds
 * pass reported no image (unreadable, or not a format the platform decodes).
 */
private fun decodeOptions(
    bounds: BitmapFactory.Options,
    size: DecodeSize,
): BitmapFactory.Options? {
    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
    return BitmapFactory.Options().apply {
        inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight, size)
        inPreferredConfig = Bitmap.Config.ARGB_8888
    }
}

/**
 * The power-of-two downsample factor for decoding a [width] × [height] source at
 * [size]: the largest factor that keeps the decoded longer edge at or above the
 * target, so the decode is the cheapest one that still fills the slot. Native
 * decodes, and slots whose target is non-positive (measured before layout gave
 * them bounds), read the source at full resolution.
 */
internal fun sampleSize(
    width: Int,
    height: Int,
    size: DecodeSize,
): Int {
    val target = (size as? DecodeSize.FitTo)?.pixels
    if (target == null || target <= 0) return 1
    val longerEdge = maxOf(width, height)
    var sample = 1
    while (longerEdge / (sample * 2) >= target) {
        sample *= 2
    }
    return sample
}

/**
 * [bitmap] rotated and flipped to the upright orientation [exif] records. Scans
 * and booklet photos are real-world JPEGs, so their pixels are often stored
 * sideways with the camera's orientation tag saying so. Returns the input
 * unchanged when it is already upright.
 */
private fun oriented(
    bitmap: Bitmap,
    exif: ExifInterface,
): Bitmap {
    val orientation =
        exif.getAttributeInt(
            ExifInterface.TAG_ORIENTATION,
            ExifInterface.ORIENTATION_NORMAL,
        )
    val upright = uprightTransform(orientation) ?: return bitmap
    val rotated =
        Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, upright.matrix(), true)
    if (rotated !== bitmap) {
        bitmap.recycle()
    }
    return rotated
}

private const val QUARTER_TURN_DEGREES = 90f
private const val HALF_TURN_DEGREES = 180f
private const val THREE_QUARTER_TURN_DEGREES = 270f

/** What brings a stored image upright: a clockwise rotation, then a mirroring
 *  across the vertical axis for the orientations whose pixels are flipped too. A
 *  vertical flip is that same mirroring about the half-turned image. */
private data class Upright(
    val degrees: Float,
    val mirrored: Boolean,
) {
    fun matrix(): Matrix =
        Matrix().apply {
            postRotate(degrees)
            if (mirrored) {
                postScale(-1f, 1f)
            }
        }
}

/** How the pixels an [orientation] tag describes must be transformed to sit
 *  upright, or null when they already do (normal, or a tag nothing recognizes). */
private fun uprightTransform(orientation: Int): Upright? =
    when (orientation) {
        ExifInterface.ORIENTATION_ROTATE_90 -> Upright(QUARTER_TURN_DEGREES, mirrored = false)
        ExifInterface.ORIENTATION_ROTATE_180 -> Upright(HALF_TURN_DEGREES, mirrored = false)
        ExifInterface.ORIENTATION_ROTATE_270 -> Upright(THREE_QUARTER_TURN_DEGREES, mirrored = false)
        ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> Upright(0f, mirrored = true)
        ExifInterface.ORIENTATION_FLIP_VERTICAL -> Upright(HALF_TURN_DEGREES, mirrored = true)
        ExifInterface.ORIENTATION_TRANSPOSE -> Upright(QUARTER_TURN_DEGREES, mirrored = true)
        ExifInterface.ORIENTATION_TRANSVERSE -> Upright(THREE_QUARTER_TURN_DEGREES, mirrored = true)
        else -> null
    }

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
import java.io.File

private const val TAG = "bae.ImageStore"
private val logger = BaeLogger(TAG)

/**
 * What an image slot shows, and therefore where its bytes come from. Every image
 * in the app is one of these five; a view names the content and renders whatever
 * [ImageStore] hands back.
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

    /** Provider art (Cover Art Archive, Discogs) that isn't in the library.
     *  Fetched through core, which owns every socket the app opens. */
    data class Remote(
        val url: String,
    ) : ImageContent

    /** A file on disk the user is previewing before it enters the library — an
     *  import candidate's cover or folder image. */
    data class LocalFile(
        val path: String,
    ) : ImageContent

    /** Bytes already in hand. Decoded on demand and never cached: the caller holds
     *  the only identity these bytes have, so this is identity-compared by
     *  reference rather than by content. */
    class Bytes(
        val bytes: ByteArray,
    ) : ImageContent

    /** Human-readable description for failure logs. */
    val description: String
        get() =
            when (this) {
                is LibraryImage -> "library image: ${image.imageType} ${image.id}"
                is ReleaseImage -> "release image: $releaseId"
                is Remote -> "remote image: $url"
                is LocalFile -> "image at path: $path"
                is Bytes -> "in-memory image: ${bytes.size} bytes"
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

/** The decoded cache's four independent budgets, one per content kind. */
enum class ImageBucket {
    LIBRARY_IMAGE,
    RELEASE_IMAGE,
    REMOTE,
    LOCAL_FILE,
}

/** Which cache the decodes of this content live in. Derived here so callers never
 *  pass a bucket. `Bytes` is never cached, but every content still names a bucket
 *  so the lookup needs no second "is this cacheable" branch — its key is null,
 *  which is what keeps it out. */
private val ImageContent.bucket: ImageBucket
    get() =
        when (this) {
            is ImageContent.LibraryImage -> ImageBucket.LIBRARY_IMAGE
            is ImageContent.ReleaseImage -> ImageBucket.RELEASE_IMAGE
            is ImageContent.Remote -> ImageBucket.REMOTE
            is ImageContent.LocalFile, is ImageContent.Bytes -> ImageBucket.LOCAL_FILE
        }

private const val MEGABYTE = 1024 * 1024

/**
 * Per-kind byte budgets for the decoded cache. Eviction never crosses buckets, so
 * a native-size release image cannot evict the album grid's covers.
 *
 * The phone numbers are the mobile column of the shared design. Local files are
 * import candidates and Android has no import flow, so nothing ever enters that
 * bucket; it carries a budget anyway because the kind exists on the contract.
 */
data class DecodedImageBudgets(
    val libraryImage: Int = 48 * MEGABYTE,
    val releaseImage: Int = 16 * MEGABYTE,
    val remote: Int = 8 * MEGABYTE,
    val localFile: Int = 16 * MEGABYTE,
)

/**
 * Per-kind byte budgets for the encoded-byte cache that sits in front of the
 * bridge. Android keeps this extra layer because an FFI crossing (plus, for a
 * release image, a cloud download and decrypt) dominates the cost of re-showing
 * an image the app has already read.
 *
 * Only the two coven-backed kinds are held, and only because their tokens carry
 * content identity — a version that moves with the bytes, an immutable file id —
 * so a held entry can never come to name different bytes. Provider art is keyed
 * on nothing but its URL, and only core's fetch knows when the bytes behind it
 * moved, so holding those bytes here would mean serving stale art indefinitely;
 * every remote read goes to core, whose own URL cache answers it. A local file is
 * already a cheap disk read.
 */
data class ImageByteBudgets(
    val libraryImage: Int = 16 * MEGABYTE,
    val releaseImage: Int = 8 * MEGABYTE,
)

/**
 * The app's image pipeline: bytes → decode at the slot's pixel size → bounded
 * decoded cache → synchronous first-frame read. One instance per open library,
 * read from the composition through [LocalImageStore]; views hold no fetch,
 * cache, or decode logic of their own.
 *
 * What a cached decode is pinned to — its token — is the content's identity, so
 * no entry can outlive the bytes it came from: a curated image keys on its
 * `_updated_at` version, a release file on its file id (immutable — an import
 * mints a fresh id per file, and a re-import mints new ones rather than
 * repointing an existing row), provider art on its URL plus the validator core
 * returns with the bytes, and a local file on its path and modification time.
 */
class ImageStore(
    /** Bytes of a curated library image, or null when no such image exists. */
    private val fetchLibraryImageBytes: suspend (image: BridgeImageRef) -> ByteArray? = { null },
    /** Bytes of one of a release's image-strip slots, downloaded from the
     *  release's cloud home (and decrypted) when it isn't on disk here. */
    private val fetchReleaseImageBytes:
        suspend (releaseId: String, source: BridgeGallerySource) -> ByteArray =
        { _, _ -> throw ImageStoreUnavailable() },
    /** Bytes of provider art at a URL, with the validator identifying them. The
     *  fetch behind it backs the desktop import flow and is absent from the
     *  Android bindings, so this keeps its throwing default here. */
    private val fetchRemoteImage: suspend (url: String) -> RemoteImageBytes =
        { throw ImageStoreUnavailable() },
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
    private val remoteValidators = RemoteValidators()

    /**
     * The already-decoded bitmap for [content] at this size, or null when it isn't
     * cached. Synchronous, so a view can draw the real art on its very first frame
     * after (re)mounting instead of flashing a placeholder while an async load
     * lands a frame later.
     *
     * The only I/O is the `stat` a local file's modification time needs; every
     * other kind is a pure cache lookup, and [ImageContent.Bytes] is never cached.
     */
    fun cachedImage(
        content: ImageContent,
        size: DecodeSize,
    ): Bitmap? {
        val key = cacheKey(content, size) ?: return null
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
        if (key != null) {
            decoded[content.bucket].get(key)?.let { return it }
        }
        val bitmap =
            withContext(dispatcher) {
                when (content) {
                    is ImageContent.LocalFile -> decodeFile(content.path, size)
                    else -> imageBytes(content)?.let { decodeBytes(it, size) }
                }
            } ?: return null
        // A key is absent only for `Bytes`, whose identity the caller holds —
        // there is nothing to pin a cache entry to — and for a local file whose
        // modification time can't be read.
        if (key != null) {
            decoded[content.bucket].put(key, bitmap)
            if (content is ImageContent.Remote) {
                remoteValidators.record(key = key, url = content.url)
            }
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
    suspend fun imageBytes(content: ImageContent): ByteArray? {
        if (content is ImageContent.Bytes) return content.bytes
        if (content is ImageContent.LocalFile) {
            return withContext(dispatcher) { File(content.path).readBytes() }
        }
        val token = token(content)
        val cache = bytes[content.bucket]
        if (token != null) {
            cache?.get(token)?.let { return it }
        }
        val fetched = withContext(dispatcher) { fetchBytes(content) } ?: return null
        if (token != null && cache != null) {
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

            is ImageContent.Remote -> {
                val fetched = fetchRemoteImage(content.url)
                dropDecodesPredating(validator = fetched.validator, url = content.url)
                fetched.bytes
            }

            // Both carry their bytes without asking core; imageBytes answers them
            // before reaching here.
            is ImageContent.LocalFile, is ImageContent.Bytes -> {
                error("${content.description} resolves its own bytes")
            }
        }

    /**
     * Evict every decode of [url] made from an older validator. Core's byte cache
     * decides when a URL's bytes are re-read; when they come back different, the
     * decodes taken from the old ones are stale at *every* pixel size, not just
     * the one being loaded.
     */
    private fun dropDecodesPredating(
        validator: String,
        url: String,
    ) {
        remoteValidators.adopt(validator = validator, url = url).forEach { stale ->
            decoded[ImageBucket.REMOTE].remove(stale)
        }
    }

    /**
     * Cache key for a decoded bitmap: its content identity plus the decode
     * resolution, so the now-playing bar's 48dp decode never serves the album
     * header's 140dp slot, and vice versa. Null when the content has no cacheable
     * identity.
     */
    private fun cacheKey(
        content: ImageContent,
        size: DecodeSize,
    ): String? = token(content)?.let { "$it#${size.keySuffix}" }

    /** What pins a cached entry to the exact bytes it came from. Null for
     *  [ImageContent.Bytes] (no identity to pin to) and for a local file whose
     *  modification time can't be read. */
    private fun token(content: ImageContent): String? =
        when (content) {
            is ImageContent.LibraryImage -> {
                libraryToken(content.image)
            }

            is ImageContent.ReleaseImage -> {
                when (val source = content.source) {
                    // A cover slot IS the release's curated cover; its version
                    // moves whenever the bytes do.
                    is BridgeGallerySource.Cover -> libraryToken(source.image)

                    // A release file's bytes are immutable per id: an import mints
                    // a fresh id per file and a re-import mints new ones, so an id
                    // never comes to name different bytes.
                    is BridgeGallerySource.ReleaseFile -> "file:${source.fileId}"
                }
            }

            is ImageContent.Remote -> {
                "remote:${content.url}"
            }

            is ImageContent.LocalFile -> {
                // File.lastModified() answers 0 both for a missing file and for one
                // whose time can't be read; either way there is no time to pin an
                // entry to, so nothing is cached and the load path surfaces the
                // read failure itself.
                val modified = File(content.path).lastModified()
                if (modified == 0L) {
                    logger.debug("no modification time for ${content.path}; not caching its decode")
                    null
                } else {
                    "path:${content.path}#$modified"
                }
            }

            is ImageContent.Bytes -> {
                null
            }
        }

    private fun libraryToken(image: BridgeImageRef): String = "library:${image.imageType.name}:${image.id}:${image.version}"

    companion object {
        /** A store that resolves nothing — for @Preview composables and the
         *  screenshot scenes, which have no open library to read from. Every slot
         *  settles on its placeholder. */
        fun unresolved(): ImageStore = ImageStore()
    }
}

/**
 * Provider-art bytes plus the token identifying this exact content: the response's
 * `ETag`, or a hash of the bytes when it carries none.
 *
 * Mirrors the bridge's `BridgeRemoteImage`, which only the desktop bindings
 * export — the fetch behind it is desktop-only, and this store compiles for
 * Android.
 */
data class RemoteImageBytes(
    val bytes: ByteArray,
    val validator: String,
) {
    // ByteArray's identity equality would make two reads of the same art unequal;
    // the validator is exactly the token that says whether they are the same
    // content, so compare on it.
    override fun equals(other: Any?): Boolean = this === other || (other is RemoteImageBytes && validator == other.validator)

    override fun hashCode(): Int = validator.hashCode()
}

/** A capability this [ImageStore] isn't wired for — the preview instance, or the
 *  desktop-only provider-art fetch. Throwing surfaces the misuse instead of
 *  masking it with empty bytes. */
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

/** The four per-kind decoded caches, addressed by bucket. Cost is the bitmap's
 *  own allocation, so a budget is a real memory ceiling. */
private class DecodedCaches(
    budgets: DecodedImageBudgets,
) {
    private val caches =
        mapOf(
            ImageBucket.LIBRARY_IMAGE to bitmapCache(budgets.libraryImage),
            ImageBucket.RELEASE_IMAGE to bitmapCache(budgets.releaseImage),
            ImageBucket.REMOTE to bitmapCache(budgets.remote),
            ImageBucket.LOCAL_FILE to bitmapCache(budgets.localFile),
        )

    operator fun get(bucket: ImageBucket): LruCache<String, Bitmap> = checkNotNull(caches[bucket]) { "every bucket is built in init" }

    private fun bitmapCache(maxBytes: Int) =
        object : LruCache<String, Bitmap>(maxBytes) {
            override fun sizeOf(
                key: String,
                value: Bitmap,
            ): Int = value.allocationByteCount
        }
}

/** The per-kind encoded-byte caches. Buckets whose absence is deliberate are the
 *  ones [ImageByteBudgets] names: remote art, local files, and in-hand bytes. */
private class ByteCaches(
    budgets: ImageByteBudgets,
) {
    private val caches =
        mapOf(
            ImageBucket.LIBRARY_IMAGE to byteCache(budgets.libraryImage),
            ImageBucket.RELEASE_IMAGE to byteCache(budgets.releaseImage),
        )

    operator fun get(bucket: ImageBucket): LruCache<String, ByteArray>? = caches[bucket]

    private fun byteCache(maxBytes: Int) =
        object : LruCache<String, ByteArray>(maxBytes) {
            override fun sizeOf(
                key: String,
                value: ByteArray,
            ): Int = value.size
        }
}

/**
 * The validator each remote URL's cached decodes were made from, and the keys they
 * live under. [LruCache] can't enumerate its keys, so the store tracks what it
 * wrote in order to drop a URL's decodes when its bytes change.
 */
private class RemoteValidators {
    private class Entry(
        var validator: String?,
        val keys: MutableSet<String>,
    )

    private val entries = HashMap<String, Entry>()

    /** Note that [url]'s decode at this size lives under [key]. */
    @Synchronized
    fun record(
        key: String,
        url: String,
    ) {
        entries.getOrPut(url) { Entry(validator = null, keys = mutableSetOf()) }.keys.add(key)
    }

    /**
     * Adopt the validator a fetch just returned for [url], and hand back the keys
     * holding decodes of the previous one. Empty when the validator is unchanged,
     * or when this is the first fetch for the URL.
     */
    @Synchronized
    fun adopt(
        validator: String,
        url: String,
    ): Set<String> {
        val entry = entries.getOrPut(url) { Entry(validator = null, keys = mutableSetOf()) }
        val previous = entry.validator
        entry.validator = validator
        if (previous == null || previous == validator) return emptySet()
        val stale = entry.keys.toSet()
        entry.keys.clear()
        return stale
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
): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    val options = decodeOptions(bounds, size) ?: return null
    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: return null
    return oriented(bitmap, ExifInterface(ByteArrayInputStream(bytes)))
}

/**
 * Decode the file at [path] the same way [decodeBytes] decodes in-memory bytes.
 * Reads through [BitmapFactory] rather than loading the file whole, so a large
 * image never sits in memory at full size just to be downsampled.
 */
internal fun decodeFile(
    path: String,
    size: DecodeSize,
): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(path, bounds)
    val options = decodeOptions(bounds, size) ?: return null
    val bitmap = BitmapFactory.decodeFile(path, options) ?: return null
    return oriented(bitmap, ExifInterface(path))
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
    val target = (size as? DecodeSize.FitTo)?.pixels ?: return 1
    if (target <= 0) return 1
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
    val matrix =
        when (
            exif.getAttributeInt(
                ExifInterface.TAG_ORIENTATION,
                ExifInterface.ORIENTATION_NORMAL,
            )
        ) {
            ExifInterface.ORIENTATION_ROTATE_90 -> {
                Matrix().apply { postRotate(90f) }
            }

            ExifInterface.ORIENTATION_ROTATE_180 -> {
                Matrix().apply { postRotate(180f) }
            }

            ExifInterface.ORIENTATION_ROTATE_270 -> {
                Matrix().apply { postRotate(270f) }
            }

            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> {
                Matrix().apply { postScale(-1f, 1f) }
            }

            ExifInterface.ORIENTATION_FLIP_VERTICAL -> {
                Matrix().apply { postScale(1f, -1f) }
            }

            ExifInterface.ORIENTATION_TRANSPOSE -> {
                Matrix().apply {
                    postRotate(90f)
                    postScale(-1f, 1f)
                }
            }

            ExifInterface.ORIENTATION_TRANSVERSE -> {
                Matrix().apply {
                    postRotate(270f)
                    postScale(-1f, 1f)
                }
            }

            else -> {
                return bitmap
            }
        }
    val rotated =
        Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
    if (rotated !== bitmap) {
        bitmap.recycle()
    }
    return rotated
}

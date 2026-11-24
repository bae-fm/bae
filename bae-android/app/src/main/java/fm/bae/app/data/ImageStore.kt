package fm.bae.app.data

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.util.LruCache
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.exifinterface.media.ExifInterface
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeCoverImageSource
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryImageType
import java.io.ByteArrayInputStream
import kotlin.coroutines.cancellation.CancellationException

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

    /** A person's avatar at the version their profile names: a new picture is a new version, so a cached
     *  decode never outlives the bytes it came from. */
    data class Avatar(
        val person: String,
        val version: String,
    ) : ImageContent

    /** The picture the viewer zooms, as core names it for its tiles; null for an artist's image or an avatar, which
     *  no viewer zooms. */
    val tiledSource: BridgeCoverImageSource?
        get() =
            when (this) {
                is LibraryImage -> {
                    BridgeCoverImageSource.Library(image).takeIf { image.imageType == BridgeLibraryImageType.COVER }
                }

                is ReleaseImage -> {
                    BridgeCoverImageSource.ReleaseImage(releaseId, source)
                }

                is Avatar -> {
                    null
                }
            }

    /** Human-readable description for failure logs. */
    val description: String
        get() =
            when (this) {
                is LibraryImage -> "library image: ${image.imageType} ${image.id}"
                is ReleaseImage -> "release image: $releaseId"
                is Avatar -> "avatar of: $person"
            }
}

/**
 * How much of the source a decode reads.
 *
 * [FitTo] takes the downsampled path, decoding the smallest power-of-two
 * reduction whose longer edge still covers the slot. [Native] decodes at the
 * source's own resolution: the viewer's tiles, which core cuts at most 512
 * pixels on a side, decode this way, and a library image is read in its largest
 * copy.
 */
sealed interface DecodeSize {
    data class FitTo(
        val pixels: Int,
    ) : DecodeSize {
        /** The part of a cache key that says at what resolution the entry was
         *  decoded, so one slot's decode never serves another's. */
        val keySuffix: String get() = pixels.toString()
    }

    data object Native : DecodeSize
}

private const val MEGABYTE = 1024 * 1024

/** What the decoded cache holds at most, in bitmap bytes, for every kind of content together. */
private const val DECODED_IMAGE_BUDGET = 64 * MEGABYTE

/** How many decodes run at once. */
private const val DECODE_WIDTH = 3

/**
 * A dispatcher for picture decodes: at most [DECODE_WIDTH] at a time, apart from
 * the threads that draw and the ones other coroutines share. Each [ImageStore]
 * makes its own.
 */
@OptIn(ExperimentalCoroutinesApi::class)
internal fun boundedDecodeDispatcher(): CoroutineDispatcher = Dispatchers.Default.limitedParallelism(DECODE_WIDTH)

/**
 * The app's image pipeline: bytes → decode at the slot's pixel size on a bounded
 * dispatcher → one bounded decoded cache → synchronous first-frame read. One instance per open library,
 * read from the composition through [LocalImageStore]; views hold no fetch,
 * cache, or decode logic of their own.
 *
 * What a cached decode is pinned to — its token — is the content's identity, so
 * no entry can outlive the bytes it came from: a curated image keys on its
 * version (the coven file its row names, which also moves when a file on
 * another device only is stored, so a failed read is asked again), and a release
 * file on its file id (immutable — an import mints a fresh id per file, and a
 * re-import mints new ones rather than repointing an existing row).
 */
class ImageStore(
    /** Bytes of a curated library image for a slot [pixels] wide on its longer
     *  side (null: the largest copy), or null when no such image exists. Core picks
     *  the cover's browse or large copy. */
    private val fetchLibraryImageBytes: suspend (image: BridgeImageRef, pixels: UInt?) -> ByteArray? = { _, _ -> null },
    /** Bytes of one of a release's image-strip slots for a slot [pixels] wide on
     *  its longer side: a cover's copy, or a release image's preview; a size above
     *  the largest copy is the picture itself, which core downloads from the
     *  release's storage (and decrypts) when it isn't on disk here. */
    private val fetchReleaseImageBytes:
        suspend (releaseId: String, source: BridgeGallerySource, pixels: UInt) -> ByteArray =
        { _, _, _ -> throw ImageStoreUnavailable() },
    /** Bytes of a person's avatar, or null when they have none. */
    private val fetchAvatarBytes: suspend (person: String) -> ByteArray? = { null },
    /** What the decoded cache holds at most, in bitmap bytes. */
    decodedBudget: Int = DECODED_IMAGE_BUDGET,
    /** Where fetches and decodes run. Injected so tests drive them on their own
     *  scheduler. */
    private val dispatcher: CoroutineDispatcher = boundedDecodeDispatcher(),
    /** Turns bytes into a bitmap at a size, on a thread of [dispatcher]. Injected so a
     *  test can see where decodes run. */
    private val decode: (ByteArray, DecodeSize) -> Bitmap? = ::decodeBytes,
) {
    constructor(handle: AppHandleInterface) : this(
        fetchLibraryImageBytes = { image, pixels -> handle.fetchLibraryImageBytes(image, pixels) },
        fetchReleaseImageBytes = { releaseId, source, pixels ->
            handle.fetchReleaseImageBytes(releaseId, source, pixels)
        },
        fetchAvatarBytes = { handle.avatar(it) },
    )

    private val decoded = bitmapCache(decodedBudget)
    private val loads = SharedLoads<Bitmap?>(CoroutineScope(SupervisorJob() + dispatcher))

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
        size: DecodeSize.FitTo,
    ): Bitmap? = decoded.get(cacheKey(content, size))

    /**
     * Decoded bitmap for [content] at [size]: the cached decode when there is one,
     * otherwise fetch the bytes, decode them off the main thread at that size, and
     * store the result. Returns null when no such image exists (a library image
     * with no bytes); a fetch failure throws, and so do bytes that aren't an
     * image this device decodes ([UnreadableImage]), rather than resolving to a
     * blank slot.
     *
     * Requests for the same content and size share one load. When the last of them
     * is cancelled, the load is cancelled too unless its decode has started: a
     * decode can't be interrupted, so the next request for it joins that decode
     * instead of starting another.
     */
    suspend fun image(
        content: ImageContent,
        size: DecodeSize.FitTo,
    ): Bitmap? {
        val key = cacheKey(content, size)
        decoded.get(key)?.let { return it }
        return loads.join(key) { control ->
            decoded.get(key) ?: loadImage(content, size, key, control)
        }
    }

    private suspend fun loadImage(
        content: ImageContent,
        size: DecodeSize.FitTo,
        key: String,
        control: LoadControl,
    ): Bitmap? {
        val bytes = withContext(dispatcher) { fetchBytes(content, size.pixels.toUInt()) } ?: return null
        control.beginDecoding()
        val bitmap =
            withContext(dispatcher) {
                decode(bytes, size) ?: throw UnreadableImage(content.description, bytes.size)
            }
        decoded.put(key, bitmap)
        return bitmap
    }

    /**
     * The encoded bytes of [art], read across the bridge. Null when no such
     * image exists.
     *
     * Consumers that hand encoded bytes to something else — the artwork
     * `ContentProvider` streaming to a media-browse client — read them here rather
     * than opening a second path to core. Core returns the largest copy of a cover.
     */
    suspend fun imageBytes(art: ImageContent.LibraryImage) = withContext(dispatcher) { fetchBytes(art, null) }

    /**
     * Read [content]'s bytes from core for a slot [pixels] wide on its longer side. Null when core reports no such
     * image.
     */
    private suspend fun fetchBytes(
        content: ImageContent,
        pixels: UInt?,
    ): ByteArray? =
        when (content) {
            is ImageContent.LibraryImage -> {
                fetchLibraryImageBytes(content.image, pixels).also {
                    if (it == null) {
                        // Core handed us an image reference but its file read
                        // returned nothing — note it rather than blanking silently.
                        logger.warning("no bytes for ${content.description}")
                    }
                }
            }

            is ImageContent.ReleaseImage -> {
                val slot = checkNotNull(pixels) { "a gallery slot has a size" }
                fetchReleaseImageBytes(content.releaseId, content.source, slot)
            }

            is ImageContent.Avatar -> {
                fetchAvatarBytes(content.person)
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

            // A profile names a new avatar version whenever its bytes change.
            is ImageContent.Avatar -> {
                "avatar:${content.person}:${content.version}"
            }
        }

    companion object {
        /** A store that resolves nothing — for the screenshot scenes, which have no
         *  open library to read from. Every slot settles on its placeholder. */
        fun unresolved(): ImageStore = ImageStore()
    }
}

/** What a curated library image's cached entries are pinned to: its kind, its
 *  subject, and the version that moves whenever its bytes do. */
private val BridgeImageRef.libraryToken: String
    get() = "library:${imageType.name}:$id:$version"

/** Bytes core handed back for an image that the platform can't decode as one. */
class UnreadableImage(
    description: String,
    byteCount: Int,
) : Exception("$description: $byteCount bytes that aren't an image this device decodes")

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

/** A cache of decoded bitmaps. Cost is the bitmap's own allocation, so a budget is a
 *  real memory ceiling. */
private fun bitmapCache(maxBytes: Int) =
    object : LruCache<String, Bitmap>(maxBytes) {
        override fun sizeOf(
            key: String,
            value: Bitmap,
        ): Int = value.allocationByteCount
    }

/**
 * How far one load has come, which decides whether the last request to leave it may
 * cancel it. A load still fetching is cancelled; one whose decode has started stays.
 */
private class LoadControl {
    private var decoding = false
    private var cancelled = false

    /** The decode is about to start: throws when the load was cancelled first. */
    @Synchronized
    fun beginDecoding() {
        if (cancelled) throw CancellationException("the load was cancelled before its decode started")
        decoding = true
    }

    /** Cancel the load unless its decode has started; true when cancelled. */
    @Synchronized
    fun cancelUnlessDecoding(): Boolean {
        if (decoding) return false
        cancelled = true
        return true
    }
}

/**
 * Loads that requests for one key share. A load is forgotten when it ends, and a
 * later request finds its result in the cache or, after a failure, starts a new
 * load.
 */
private class SharedLoads<T>(
    private val scope: CoroutineScope,
) {
    private class Load<T>(
        val control: LoadControl,
    ) {
        lateinit var result: Deferred<T>
        var waiters = 0
    }

    private val lock = Any()
    private val loads = HashMap<String, Load<T>>()

    /** The result of the load for [key], started with [operation] when none runs. */
    suspend fun join(
        key: String,
        operation: suspend (LoadControl) -> T,
    ): T {
        val load = enter(key, operation)
        try {
            return load.result.await()
        } finally {
            leave(key, load)
        }
    }

    private fun enter(
        key: String,
        operation: suspend (LoadControl) -> T,
    ): Load<T> =
        synchronized(lock) {
            val load =
                loads.getOrPut(key) {
                    Load<T>(LoadControl()).also { created ->
                        created.result =
                            scope.async(start = CoroutineStart.LAZY) {
                                try {
                                    operation(created.control)
                                } finally {
                                    ended(key, created)
                                }
                            }
                    }
                }
            load.waiters++
            load
        }

    private fun ended(
        key: String,
        load: Load<T>,
    ) = synchronized(lock) {
        if (loads[key] === load) loads.remove(key)
    }

    /** The request's wait is over, with its answer or cancelled. The last request to
     *  leave cancels the load, unless its decode has started. */
    private fun leave(
        key: String,
        load: Load<T>,
    ) = synchronized(lock) {
        load.waiters--
        if (load.waiters == 0 && load.control.cancelUnlessDecoding()) {
            if (loads[key] === load) loads.remove(key)
            load.result.cancel()
        }
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

package fm.bae.app.ui.components

import android.util.LruCache
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import coil3.compose.AsyncImage
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val TAG = "bae.CoverImage"
private val logger = BaeLogger(TAG)

/**
 * The bytes of recently-shown covers, keyed on [cacheKey] so a grid doesn't
 * re-cross the bridge for the same cover as it scrolls. Covers are re-fetchable
 * by id, so eviction just reloads; the cap is in bytes (via [LruCache.sizeOf]),
 * not entry count, since a single cover can be a few MB. One instance is built at
 * the composition root and handed to cover leaves through [LocalCoverBytesCache].
 */
class CoverBytesCache {
    private val cache =
        object : LruCache<String, ByteArray>(MAX_BYTES) {
            override fun sizeOf(
                key: String,
                value: ByteArray,
            ): Int = value.size
        }

    fun get(key: String): ByteArray? = cache.get(key)

    fun put(
        key: String,
        bytes: ByteArray,
    ) {
        cache.put(key, bytes)
    }

    private companion object {
        const val MAX_BYTES = 32 * 1024 * 1024
    }
}

/**
 * The cover-bytes cache for the open app, provided once at the composition root.
 * No default: rendering a cover outside that provider is a wiring bug, so it
 * fails loudly rather than handing out a throwaway per-call cache.
 */
val LocalCoverBytesCache =
    staticCompositionLocalOf<CoverBytesCache> {
        error("LocalCoverBytesCache not provided")
    }

/**
 * The dispatcher cover and gallery byte fetches run on. Defaults to
 * [Dispatchers.IO]; a test overrides it through the composition local to drive
 * the fetches on its own scheduler.
 */
val LocalImageDispatcher =
    staticCompositionLocalOf<CoroutineDispatcher> { Dispatchers.IO }

/**
 * A cover's cache identity. A version (the image row's `_updated_at`) busts the
 * cache when the cover is replaced in place; now-playing/queue covers carry only
 * an id (no version), so they key on the id alone.
 */
private fun cacheKey(
    id: String,
    version: String?,
): String = if (version != null) "$id#$version" else id

private sealed interface CoverState {
    object Loading : CoverState

    object Absent : CoverState

    class Loaded(
        val bytes: ByteArray,
        val cacheKey: String,
    ) : CoverState
}

/**
 * Resolve a cover reference (image id + optional version) to its bytes: a cache
 * hit renders immediately and never re-crosses the bridge; a miss fetches via
 * [loadImage] and caches the result. A null id, an absent cover, or a failed
 * fetch resolves to [CoverState.Absent] (the loss is logged, never silent).
 */
@Composable
private fun rememberCoverState(
    coverId: String?,
    coverVersion: String?,
    loadImage: suspend (imageId: String) -> ByteArray?,
): CoverState {
    val coverCache = LocalCoverBytesCache.current
    val dispatcher = LocalImageDispatcher.current
    val key = coverId?.let { cacheKey(it, coverVersion) }
    var state by remember(key) {
        mutableStateOf(
            when {
                key == null -> CoverState.Absent
                else -> coverCache.get(key)?.let { CoverState.Loaded(it, key) } ?: CoverState.Loading
            },
        )
    }
    LaunchedEffect(key) {
        if (key == null || state is CoverState.Loaded) return@LaunchedEffect
        state =
            try {
                val bytes = withContext(dispatcher) { loadImage(coverId) }
                if (bytes == null) {
                    // Core handed us a cover reference but its blob read returned
                    // nothing — unexpected, so note it rather than blanking silently.
                    logger.warning("cover image bytes absent for $coverId")
                    CoverState.Absent
                } else {
                    coverCache.put(key, bytes)
                    CoverState.Loaded(bytes, key)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("Failed to load cover image $coverId", e)
                CoverState.Absent
            }
    }
    return state
}

/**
 * Album-cover thumbnail: the cover for [coverId] (fetched by id and cached by
 * `(id, version)`) clipped to a rounded square, or a MusicNote placeholder when
 * there is no cover. The caller's [modifier] carries the sizing —
 * `Modifier.size(48.dp)` for list rows, `Modifier.fillMaxWidth().aspectRatio(1f)`
 * for full-width art. [loadImage] reads cover bytes off the bridge by release id.
 */
@Composable
fun CoverImage(
    coverId: String?,
    coverVersion: String?,
    loadImage: suspend (imageId: String) -> ByteArray?,
    cornerRadius: Dp,
    iconPadding: Dp,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
) {
    Box(
        modifier = modifier.clip(RoundedCornerShape(cornerRadius)),
        contentAlignment = Alignment.Center,
    ) {
        when (val state = rememberCoverState(coverId, coverVersion, loadImage)) {
            is CoverState.Loaded -> {
                AsyncImage(
                    model = coverModel(state.bytes, state.cacheKey),
                    contentDescription = contentDescription,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }

            // A cover exists but its bytes aren't in yet: a plain tile (no glyph),
            // so the art pops in without a placeholder flash beforehand.
            CoverState.Loading -> {
                CoverTile(showIcon = false, iconPadding = iconPadding)
            }

            // No cover, or its bytes were absent/failed (logged in rememberCoverState).
            CoverState.Absent -> {
                CoverTile(showIcon = true, iconPadding = iconPadding)
            }
        }
    }
}

/**
 * A Coil model for in-memory cover [bytes], keyed in the memory cache on
 * [cacheKey] so a recomposition (a scroll back to a visible card) reuses the
 * decoded bitmap instead of decoding again.
 */
@Composable
private fun coverModel(
    bytes: ByteArray,
    cacheKey: String,
): ImageRequest {
    val context = LocalPlatformContext.current
    return remember(cacheKey) {
        ImageRequest
            .Builder(context)
            .data(bytes)
            .memoryCacheKey(cacheKey)
            .build()
    }
}

/** The placeholder tile behind/instead of a cover: a flat surface, with the
 *  MusicNote glyph only when there is no cover to show. */
@Composable
private fun CoverTile(
    showIcon: Boolean,
    iconPadding: Dp,
) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        modifier = Modifier.fillMaxSize(),
    ) {
        if (showIcon) {
            Icon(
                imageVector = Icons.Filled.MusicNote,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(iconPadding),
            )
        }
    }
}

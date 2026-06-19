package fm.bae.app.ui

import android.util.Log
import androidx.compose.animation.core.animate
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import coil3.request.crossfade
import coil3.size.Size
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeGalleryItem
import java.io.File

private const val TAG = "bae.GalleryDialog"

/**
 * Full-screen artwork viewer over a release's gallery items (cover first, then
 * every image file the release has). Swipeable when there's more than one. An
 * item already on disk renders from its [BridgeGalleryItem.localPath]; a
 * cloud-only item (no local path) is fetched on demand via [loadImage], keyed by
 * the item's id. Each page pinch-zooms and snaps back when you release.
 */
@Composable
fun GalleryDialog(
    items: List<BridgeGalleryItem>,
    loadImage: suspend (fileId: String) -> ByteArray,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        val pagerState = rememberPagerState(pageCount = { items.size })
        Surface(modifier = Modifier.fillMaxSize(), color = Color.Black) {
            Box(modifier = Modifier.fillMaxSize()) {
                HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
                    val item = items[page]
                    val localPath = item.localPath
                    if (localPath != null) {
                        // The full identifier (with any `#v=<mtime>` suffix) is the
                        // cache key; [coverFile] strips it to the on-disk path.
                        ZoomableGalleryImage(
                            data = coverFile(localPath),
                            cacheKey = localPath,
                            contentDescription = item.label,
                        )
                    } else {
                        RemoteGalleryImage(
                            fileId = item.id,
                            label = item.label,
                            loadImage = loadImage,
                        )
                    }
                }
                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier.align(Alignment.TopEnd).padding(8.dp),
                ) {
                    Icon(
                        imageVector = Icons.Filled.Close,
                        contentDescription = stringResource(R.string.close),
                        tint = Color.White,
                    )
                }
                // Current item's label (e.g. "Cover", "Back.jpg") + page counter,
                // so multi-image galleries aren't a blind swipe-through.
                Column(
                    modifier = Modifier.align(Alignment.BottomCenter).padding(16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    // Core always sets a non-empty label ("Cover" or the file's
                    // original filename), so render it unconditionally.
                    Text(text = items[pagerState.currentPage].label, color = Color.White)
                    if (items.size > 1) {
                        Text(
                            text = "${pagerState.currentPage + 1} / ${items.size}",
                            color = Color.White.copy(alpha = 0.7f),
                        )
                    }
                }
            }
        }
    }
}

/**
 * A pinch-zoomable gallery image. Coil downsamples to the page size for the
 * initial draw (so a huge — e.g. 35 MB — JPEG never decodes at full resolution
 * just to fill the screen); once the user pinches in, it requests the original
 * resolution so the zoomed-in detail is sharp, crossfading up from the
 * downsampled image. The pinch scales the image around the gesture centroid and
 * releases back to fit, mirroring the macOS lightbox.
 *
 * [data] is the Coil model — a [java.io.File] for an on-disk image or a
 * [ByteArray] for fetched cloud bytes. [cacheKey] keys the downsampled image in
 * the memory/disk cache (the bridge identifier for a local file, the file id for
 * fetched bytes); the full-resolution request reuses it as its placeholder so
 * the upgrade doesn't flash.
 */
@Composable
private fun ZoomableGalleryImage(
    data: Any,
    cacheKey: String,
    contentDescription: String?,
) {
    val context = LocalPlatformContext.current
    var scale by remember(cacheKey) { mutableFloatStateOf(1f) }
    var origin by remember(cacheKey) { mutableStateOf(TransformOrigin.Center) }
    var fullRes by remember(cacheKey) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    val model =
        remember(data, cacheKey, fullRes) {
            ImageRequest
                .Builder(context)
                .data(data)
                .crossfade(true)
                .apply {
                    // A local on-disk file can be disk-cached by its identifier;
                    // in-memory bytes (a fetched cloud image) aren't disk-cacheable,
                    // so the disk key only applies to the File source.
                    if (data is File) {
                        diskCacheKey(cacheKey)
                    }
                    if (fullRes) {
                        // Decode at the source's full resolution and reuse the
                        // already-cached downsampled image as the placeholder, so
                        // the zoomed view sharpens in place instead of blanking.
                        size(Size.ORIGINAL)
                        memoryCacheKey("$cacheKey#orig")
                        placeholderMemoryCacheKey(cacheKey)
                    } else {
                        // Default size resolves to the page bounds: Coil
                        // downsamples the source to fit, the cheap initial decode.
                        memoryCacheKey(cacheKey)
                    }
                }.build()
        }

    AsyncImage(
        model = model,
        contentDescription = contentDescription,
        contentScale = ContentScale.Fit,
        modifier =
            Modifier
                .fillMaxSize()
                .pointerInput(cacheKey) {
                    awaitEachGesture {
                        awaitFirstDown(requireUnconsumed = false)
                        var originSet = false
                        do {
                            val event = awaitPointerEvent()
                            val zoom = event.calculateZoom()
                            if (zoom != 1f) {
                                if (!originSet && size.width > 0 && size.height > 0) {
                                    val centroid = event.calculateCentroid()
                                    origin =
                                        TransformOrigin(
                                            centroid.x / size.width,
                                            centroid.y / size.height,
                                        )
                                    originSet = true
                                }
                                scale = (scale * zoom).coerceAtLeast(1f)
                                if (scale > 1.01f && !fullRes) {
                                    fullRes = true
                                }
                                // Consume so the pager doesn't treat a pinch as a
                                // page swipe; single-finger drags (zoom == 1) fall
                                // through to the pager.
                                event.changes.forEach { it.consume() }
                            }
                        } while (event.changes.any { it.pressed })
                        // All pointers up: snap back to fit.
                        if (scale != 1f) {
                            scope.launch {
                                animate(scale, 1f) { value, _ -> scale = value }
                            }
                        }
                    }
                }.graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    transformOrigin = origin
                },
    )
}

/**
 * A gallery image whose file isn't on disk here: fetch its bytes (downloaded
 * from the release's cloud home and decrypted by core), then hand them to
 * [ZoomableGalleryImage] to decode and display. Shows a spinner while fetching
 * and the failure reason if the fetch fails. `result` is null while loading,
 * then success(bytes) or failure(error).
 */
@Composable
private fun RemoteGalleryImage(
    fileId: String,
    label: String,
    loadImage: suspend (fileId: String) -> ByteArray,
) {
    var result: Result<ByteArray>? by remember(fileId) {
        mutableStateOf<Result<ByteArray>?>(null)
    }
    LaunchedEffect(fileId) {
        result =
            try {
                Result.success(withContext(Dispatchers.IO) { loadImage(fileId) })
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(TAG, "Failed to load gallery image $fileId", e)
                Result.failure(e)
            }
    }
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val r = result
        when {
            r == null -> {
                CircularProgressIndicator(color = Color.White)
            }

            r.isSuccess -> {
                ZoomableGalleryImage(
                    data = r.getOrThrow(),
                    cacheKey = fileId,
                    contentDescription = label,
                )
            }

            // The underlying failure is already logged above; the viewer shows a
            // fixed, user-facing message rather than a raw exception string.
            else -> {
                Text(
                    text = stringResource(R.string.gallery_load_failed),
                    color = Color.White,
                    modifier = Modifier.padding(24.dp),
                )
            }
        }
    }
}

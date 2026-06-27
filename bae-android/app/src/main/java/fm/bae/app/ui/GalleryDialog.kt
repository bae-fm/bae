package fm.bae.app.ui

import androidx.compose.animation.core.animate
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PagerState
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import coil3.request.crossfade
import coil3.size.Size
import fm.bae.app.BaeLogger
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGallerySource

private const val FULL_RES_SCALE_THRESHOLD = 1.01f
private const val DISMISS_THRESHOLD_DP = 150f
private const val DRAG_FADE_DISTANCE_MULTIPLIER = 2f
private const val DRAG_BACKDROP_FADE = 0.7f
private const val TAG = "bae.GalleryDialog"
private val logger = BaeLogger(TAG)

/**
 * Full-screen artwork viewer over a release's gallery items (cover first, then
 * every image file the release has). Swipeable when there's more than one, and a
 * downward swipe dismisses it. Each item's bytes are fetched on demand per its
 * [BridgeGalleryItem.source]: a [BridgeGallerySource.Cover] by image id via
 * [loadCover], a [BridgeGallerySource.ReleaseFile] by file id via
 * [loadGalleryFile]. Each page pinch-zooms and snaps back when you release.
 */
@Composable
fun GalleryDialog(
    items: List<BridgeGalleryItem>,
    loadCover: suspend (imageId: String) -> ByteArray?,
    loadGalleryFile: suspend (fileId: String) -> ByteArray,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        val pagerState = rememberPagerState(pageCount = { items.size })
        val scope = rememberCoroutineScope()
        var offsetY by remember { mutableFloatStateOf(0f) }
        val dismissThresholdPx = with(LocalDensity.current) { DISMISS_THRESHOLD_DP.dp.toPx() }
        // Fade the backdrop toward the dismiss threshold as the viewer is dragged
        // down, so the photo appears to lift away over the app behind it.
        val dragProgress = (offsetY / (dismissThresholdPx * DRAG_FADE_DISTANCE_MULTIPLIER)).coerceIn(0f, 1f)
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = Color.Black.copy(alpha = 1f - dragProgress * DRAG_BACKDROP_FADE),
        ) {
            // The outer (untranslated) box owns the vertical swipe-to-dismiss; the
            // inner box carries the drag offset, so the gesture's coordinate space
            // stays put. The vertical drag and the pager's horizontal swipe each
            // wait on their own axis' touch slop, so neither steals the other; a
            // pinch-zoom consumes its events first, so it doesn't dismiss either.
            Box(
                modifier =
                    Modifier.fillMaxSize().pointerInput(Unit) {
                        detectVerticalDragGestures(
                            onVerticalDrag = { change, dragAmount ->
                                offsetY = (offsetY + dragAmount).coerceAtLeast(0f)
                                change.consume()
                            },
                            onDragEnd = {
                                if (offsetY > dismissThresholdPx) {
                                    onDismiss()
                                } else {
                                    scope.launch { animate(offsetY, 0f) { value, _ -> offsetY = value } }
                                }
                            },
                            onDragCancel = {
                                scope.launch { animate(offsetY, 0f) { value, _ -> offsetY = value } }
                            },
                        )
                    },
            ) {
                Box(modifier = Modifier.fillMaxSize().graphicsLayer { translationY = offsetY }) {
                    HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
                        GalleryPage(item = items[page], loadCover = loadCover, loadGalleryFile = loadGalleryFile)
                    }
                    GalleryCloseButton(onDismiss = onDismiss)
                    GalleryCaption(items = items, pagerState = pagerState)
                }
            }
        }
    }
}

/**
 * One gallery page. A [BridgeGallerySource.Cover] fetches the cover bytes by
 * image id and keys its cache on `(id, version)` so a replaced cover reloads; a
 * [BridgeGallerySource.ReleaseFile] fetches by file id and keys on the file id.
 */
@Composable
private fun GalleryPage(
    item: BridgeGalleryItem,
    loadCover: suspend (imageId: String) -> ByteArray?,
    loadGalleryFile: suspend (fileId: String) -> ByteArray,
) {
    when (val source = item.source) {
        is BridgeGallerySource.Cover ->
            RemoteGalleryImage(
                cacheKey = "${source.image.id}#${source.image.version}",
                label = item.label,
            ) {
                // Core handed us a cover reference, so absent bytes are an error, not a
                // normal empty — surface it through the page's failure state.
                loadCover(source.image.id)
                    ?: throw IllegalStateException("cover image ${source.image.id} has no bytes")
            }

        is BridgeGallerySource.ReleaseFile ->
            RemoteGalleryImage(cacheKey = item.id, label = item.label) { loadGalleryFile(item.id) }
    }
}

/** Close button, inset out of the status bar and any display cutout. */
@Composable
private fun BoxScope.GalleryCloseButton(onDismiss: () -> Unit) {
    IconButton(
        onClick = onDismiss,
        modifier =
            Modifier
                .align(Alignment.TopEnd)
                .windowInsetsPadding(
                    WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal),
                ).padding(8.dp),
    ) {
        Icon(
            imageVector = Icons.Filled.Close,
            contentDescription = stringResource(R.string.close),
            tint = Color.White,
        )
    }
}

/**
 * Current item's label (e.g. "Cover", "Back.jpg") and, for a multi-image gallery,
 * its position — inset above the navigation bar so it isn't a blind swipe-through.
 * Reads [PagerState.currentPage] here at the leaf so only the caption recomposes
 * on a page turn, not the parent.
 */
@Composable
private fun BoxScope.GalleryCaption(
    items: List<BridgeGalleryItem>,
    pagerState: PagerState,
) {
    val currentPage = pagerState.currentPage
    Column(
        modifier =
            Modifier
                .align(Alignment.BottomCenter)
                .windowInsetsPadding(
                    WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom + WindowInsetsSides.Horizontal),
                ).padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // Core always sets a non-empty label ("Cover" or the file's original
        // filename), so render it unconditionally.
        Text(text = items[currentPage].label, color = Color.White)
        if (items.size > 1) {
            Text(
                text = "${currentPage + 1} / ${items.size}",
                color = Color.White.copy(alpha = 0.7f),
            )
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
 * [bytes] are the fetched (and decrypted) image bytes Coil decodes. [cacheKey]
 * keys the downsampled image in the memory cache; the full-resolution request
 * reuses it as its placeholder so the upgrade doesn't flash.
 */
@Composable
private fun ZoomableGalleryImage(
    bytes: ByteArray,
    cacheKey: String,
    contentDescription: String?,
) {
    val context = LocalPlatformContext.current
    var scale by remember(cacheKey) { mutableFloatStateOf(1f) }
    var origin by remember(cacheKey) { mutableStateOf(TransformOrigin.Center) }
    var fullRes by remember(cacheKey) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    val model =
        remember(bytes, cacheKey, fullRes) {
            buildGalleryImageRequest(context, bytes, cacheKey, fullRes)
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
                                if (scale > FULL_RES_SCALE_THRESHOLD && !fullRes) {
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

private fun buildGalleryImageRequest(
    context: coil3.PlatformContext,
    bytes: ByteArray,
    cacheKey: String,
    fullRes: Boolean,
): ImageRequest =
    ImageRequest
        .Builder(context)
        .data(bytes)
        .crossfade(true)
        .apply {
            // In-memory bytes (a fetched, decrypted image) aren't disk-cacheable,
            // so only the memory cache is keyed.
            if (fullRes) {
                // Decode at the source's full resolution and reuse the
                // already-cached downsampled image as the placeholder, so
                // the zoomed view sharpens in place instead of blanking.
                size(Size.ORIGINAL)
                memoryCacheKey("$cacheKey#orig")
                placeholderMemoryCacheKey(cacheKey)
            } else {
                // Default size resolves to the page bounds: Coil downsamples
                // the source to fit, the cheap initial decode.
                memoryCacheKey(cacheKey)
            }
        }.build()

/**
 * A gallery image fetched on demand (downloaded from the release's cloud home and
 * decrypted by core, or read from coven's image store for a cover), then handed
 * to [ZoomableGalleryImage] to decode and display. Shows a spinner while fetching
 * and a fixed failure message if [load] throws. [cacheKey] keys the decoded image
 * and re-runs the fetch when it changes. `result` is null while loading, then
 * success(bytes) or failure(error).
 */
@Composable
private fun RemoteGalleryImage(
    cacheKey: String,
    label: String,
    load: suspend () -> ByteArray,
) {
    val dispatcher = LocalImageDispatcher.current
    var result: Result<ByteArray>? by remember(cacheKey) {
        mutableStateOf<Result<ByteArray>?>(null)
    }
    LaunchedEffect(cacheKey) {
        result =
            try {
                Result.success(withContext(dispatcher) { load() })
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("Failed to load gallery image $cacheKey", e)
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
                    bytes = r.getOrThrow(),
                    cacheKey = cacheKey,
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

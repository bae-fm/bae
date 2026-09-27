package fm.bae.app.ui.albumdetail

import android.graphics.Bitmap
import android.os.Build
import android.view.WindowManager
import androidx.compose.animation.core.animate
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import fm.bae.app.BaeLogger
import fm.bae.app.R
import fm.bae.app.data.DecodeSize
import fm.bae.app.data.ImageContent
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeGalleryItem

private const val FULL_RES_SCALE_THRESHOLD = 1.01f
private const val DISMISS_THRESHOLD_DP = 150f
private const val DRAG_FADE_DISTANCE_MULTIPLIER = 2f
private const val DRAG_BACKDROP_FADE = 0.7f
private const val TAG = "bae.GalleryDialog"
private val logger = BaeLogger(TAG)

/**
 * Full-screen viewer over a release's gallery items: swipe between them, pinch
 * to zoom, swipe down to dismiss.
 */
@Composable
fun GalleryDialog(
    releaseId: String,
    items: List<BridgeGalleryItem>,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        // Without this the content gets no inset values and the caption renders
        // under the navigation bar.
        properties =
            DialogProperties(
                usePlatformDefaultWidth = false,
                decorFitsSystemWindows = false,
            ),
    ) {
        FullDisplayDialogWindow()
        val pagerState = rememberPagerState(pageCount = { items.size })
        val scope = rememberCoroutineScope()
        var offsetY by remember { mutableFloatStateOf(0f) }
        val dismissThresholdPx = with(LocalDensity.current) { DISMISS_THRESHOLD_DP.dp.toPx() }
        // Fade the backdrop as the viewer is dragged down toward dismissal.
        val dragProgress = (offsetY / (dismissThresholdPx * DRAG_FADE_DISTANCE_MULTIPLIER)).coerceIn(0f, 1f)
        val backdrop = BaeTheme.colors.backdrop
        Surface(
            modifier = Modifier.fillMaxSize(),
            color = backdrop.copy(alpha = backdrop.alpha * (1f - dragProgress * DRAG_BACKDROP_FADE)),
        ) {
            // The outer box handles swipe-to-dismiss and only the inner box moves,
            // so the gesture's coordinates stay fixed.
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
                        GalleryPage(releaseId = releaseId, item = items[page])
                    }
                    GalleryCloseButton(onDismiss = onDismiss)
                    GalleryCaption(items = items, pagerState = pagerState)
                }
            }
        }
    }
}

/**
 * Stretch the dialog's window over the whole display, cutout included, so its
 * bottom doesn't hang off screen and Compose gets real inset values.
 */
@Composable
private fun FullDisplayDialogWindow() {
    val window = (LocalView.current.parent as DialogWindowProvider).window
    SideEffect {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.attributes =
            window.attributes.apply {
                width = WindowManager.LayoutParams.MATCH_PARENT
                height = WindowManager.LayoutParams.MATCH_PARENT
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    layoutInDisplayCutoutMode =
                        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                }
                // Fit no system bars, so they arrive as insets instead.
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    fitInsetsTypes = 0
                } else {
                    @Suppress("DEPRECATION")
                    flags = flags or
                        WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                        WindowManager.LayoutParams.FLAG_LAYOUT_INSET_DECOR
                }
            }
    }
}

/** One gallery page: the release-image slot the item names. */
@Composable
private fun GalleryPage(
    releaseId: String,
    item: BridgeGalleryItem,
) {
    val content =
        remember(releaseId, item.id) { ImageContent.ReleaseImage(releaseId, item.source) }
    GalleryImage(content = content, label = item.label)
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
                ).padding(ThemeSpace.related),
    ) {
        Icon(
            imageVector = Icons.Filled.Close,
            contentDescription = stringResource(R.string.close),
            tint = BaeTheme.colors.onFill,
        )
    }
}

/**
 * Current item's label and, when there are several, its position. Reads
 * [PagerState.currentPage] here so only the caption recomposes on a page turn.
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
                ).padding(ThemeSpace.edge),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // Core always sets a non-empty label.
        Text(text = items[currentPage].label, color = BaeTheme.colors.onFill)
        if (items.size > 1) {
            Text(
                text = "${currentPage + 1} / ${items.size}",
                color = BaeTheme.colors.onFillSecondary,
            )
        }
    }
}

/**
 * A gallery image decoded to the page bounds, then again at full resolution once
 * the user zooms in; a spinner while loading, a failure message if it fails.
 */
@Composable
private fun GalleryImage(
    content: ImageContent,
    label: String,
) {
    val store = LocalImageStore.current
    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val fitSize = DecodeSize.FitTo(maxOf(constraints.maxWidth, constraints.maxHeight))
        var fitted by remember(content, fitSize) { mutableStateOf(store.cachedImage(content, fitSize)) }
        var native by remember(content) { mutableStateOf(store.cachedImage(content, DecodeSize.Native)) }
        var failed by remember(content) { mutableStateOf(false) }
        var zoomedIn by remember(content) { mutableStateOf(false) }

        LaunchedEffect(content, fitSize) {
            if (fitted != null) return@LaunchedEffect
            val loaded = loadOrNull(label) { store.image(content, fitSize) }
            if (loaded == null) {
                failed = true
            } else {
                fitted = loaded
            }
        }
        LaunchedEffect(content, zoomedIn) {
            if (!zoomedIn || native != null) return@LaunchedEffect
            // A failed full-resolution decode leaves the downsampled image on
            // screen rather than replacing a usable view with an error.
            native = loadOrNull(label) { store.image(content, DecodeSize.Native) }
        }

        val shown = native ?: fitted
        when {
            shown != null -> {
                ZoomableGalleryImage(
                    bitmap = shown,
                    identity = content,
                    contentDescription = label,
                    onZoomedIn = { zoomedIn = true },
                )
            }

            failed -> {
                Text(
                    text = stringResource(R.string.gallery_load_failed),
                    color = BaeTheme.colors.onFill,
                    modifier = Modifier.padding(ThemeSpace.section),
                )
            }

            else -> {
                CircularProgressIndicator(color = BaeTheme.colors.onFill)
            }
        }
    }
}

/**
 * Run [load], logging and answering null when it fails or finds no bytes;
 * cancellation propagates.
 */
private suspend fun loadOrNull(
    label: String,
    load: suspend () -> Bitmap?,
): Bitmap? =
    try {
        load().also {
            if (it == null) {
                logger.warning("no bytes for gallery image $label")
            }
        }
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        logger.error("Failed to load gallery image $label", e)
        null
    }

/**
 * [bitmap] zoomed by pinching and snapped back to fit on release; calls
 * [onZoomedIn] past the full-resolution threshold. [identity] resets the zoom.
 */
@Composable
private fun ZoomableGalleryImage(
    bitmap: Bitmap,
    identity: Any,
    contentDescription: String?,
    onZoomedIn: () -> Unit,
) {
    var scale by remember(identity) { mutableFloatStateOf(1f) }
    var origin by remember(identity) { mutableStateOf(TransformOrigin.Center) }
    val scope = rememberCoroutineScope()

    Image(
        bitmap = bitmap.asImageBitmap(),
        contentDescription = contentDescription,
        contentScale = ContentScale.Fit,
        modifier =
            Modifier
                .fillMaxSize()
                .pointerInput(identity) {
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
                                if (scale > FULL_RES_SCALE_THRESHOLD) {
                                    onZoomedIn()
                                }
                                // Consume so the pager doesn't treat a pinch as a
                                // page swipe.
                                event.changes.forEach { it.consume() }
                            }
                        } while (event.changes.any { it.pressed })
                        // Snap back to fit.
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

@Preview(showBackground = true)
@Composable
private fun GalleryDialogPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore()) {
            GalleryDialog(
                releaseId = "rel-1",
                items = listOf(PreviewData.galleryItem()),
                onDismiss = {},
            )
        }
    }
}

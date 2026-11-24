package fm.bae.app.ui.albumdetail

import android.content.Context
import android.os.Build
import android.view.WindowManager
import androidx.compose.animation.core.animate
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.traversalIndex
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import fm.bae.app.BaeLogger
import fm.bae.app.LocalBugReports
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DecodeSize
import fm.bae.app.data.ImageContent
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.IconTone
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGalleryLabel

private const val DISMISS_THRESHOLD_DP = 150f
private const val DRAG_FADE_DISTANCE_MULTIPLIER = 2f
private const val DRAG_BACKDROP_FADE = 0.7f
private const val TAG = "bae.AndroidLightbox"
private val logger = BaeLogger(TAG)

/**
 * Full-screen viewer over a release's gallery items: swipe between them, pinch
 * to zoom, swipe down to dismiss. [count] is the number of gallery items that
 * the release's query counts, and the page counter shows that number.
 */
@Composable
fun AndroidLightbox(
    releaseId: String,
    title: String,
    items: List<BridgeGalleryItem>,
    count: UInt,
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
        GalleryViewer(releaseId = releaseId, title = title, items = items, count = count, onDismiss = onDismiss)
    }
}

/**
 * The viewer inside the lightbox's window: the gallery's items on the dark backdrop, Close at the top end and the
 * caption at the foot. It holds no window, so a shot can draw it.
 */
@Composable
internal fun GalleryViewer(
    releaseId: String,
    title: String,
    items: List<BridgeGalleryItem>,
    count: UInt,
    onDismiss: () -> Unit,
) {
    val pagerState = rememberPagerState(pageCount = { items.size })
    val scope = rememberScreenScope()
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
            // The viewer is named for the slot shown, and the picture in it for the release whose art it shows.
            val shownLabel = LocalContext.current.galleryLabelText(items[pagerState.currentPage].label)
            Box(
                modifier =
                    Modifier
                        .fillMaxSize()
                        .graphicsLayer { translationY = offsetY }
                        .semantics { contentDescription = shownLabel },
            ) {
                HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
                    GalleryPage(releaseId = releaseId, title = title, item = items[page])
                }
                GalleryCloseButton(onDismiss = onDismiss)
                GalleryFoot(items = items, count = count, pagerState = pagerState)
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
    title: String,
    item: BridgeGalleryItem,
) {
    val content =
        remember(releaseId, item.id) { ImageContent.ReleaseImage(releaseId, item.source) }
    GalleryImage(content = content, label = LocalContext.current.galleryLabelText(item.label), title = title)
}

/** A gallery slot's name: the cover in the app's language, a file by its filename. */
private fun Context.galleryLabelText(label: BridgeGalleryLabel): String =
    when (label) {
        BridgeGalleryLabel.Cover -> coreString("core.gallery.cover")
        is BridgeGalleryLabel.File -> label.name
    }

/** Close button, inset out of the status bar and any display cutout. */
@Composable
private fun BoxScope.GalleryCloseButton(onDismiss: () -> Unit) {
    AndroidIconButton(
        icon = SymbolIcons.Close,
        contentDescription = stringResource(R.string.close),
        onClick = onDismiss,
        modifier =
            Modifier
                .align(Alignment.TopEnd)
                // Close is read before the picture, as the board lists it, though it is drawn over it.
                .semantics { traversalIndex = -1f }
                .windowInsetsPadding(
                    WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal),
                ).padding(ThemeSpace.inline),
        tone = IconTone.ON_FILL,
    )
}

/**
 * The gallery's foot: the shown item's caption at the bottom centre. Reads [PagerState.currentPage] here so only the
 * caption recomposes on a page turn.
 */
@Composable
private fun BoxScope.GalleryFoot(
    items: List<BridgeGalleryItem>,
    count: UInt,
    pagerState: PagerState,
) {
    val currentPage = pagerState.currentPage
    AndroidGalleryCaption(
        label = LocalContext.current.galleryLabelText(items[currentPage].label),
        page = currentPage + 1,
        count = count,
        modifier = Modifier.align(Alignment.BottomCenter),
    )
}

/**
 * A gallery image decoded to the page bounds, which zooms from core's tiles; a
 * spinner naming the load while it runs, a failure message if it fails.
 */
@Composable
private fun GalleryImage(
    content: ImageContent,
    label: String,
    title: String,
) {
    val store = LocalImageStore.current
    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val fitSize = DecodeSize.FitTo(maxOf(constraints.maxWidth, constraints.maxHeight))
        var fitted by remember(content, fitSize) { mutableStateOf(store.cachedImage(content, fitSize)) }
        var attempt by remember(content) { mutableIntStateOf(0) }
        var failure by remember(content, attempt) { mutableStateOf<GalleryFailure?>(null) }
        val unloaded = stringResource(R.string.image_load_failed)
        val context = LocalContext.current
        val bugs = LocalBugReports.current

        LaunchedEffect(content, fitSize, attempt) {
            if (fitted != null) return@LaunchedEffect
            try {
                val loaded = store.image(content, fitSize)
                if (loaded == null) {
                    logger.warning("no bytes for gallery image $label")
                    failure = GalleryFailure.NoBytes
                } else {
                    fitted = loaded
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                logger.error("Failed to load gallery image $label", e)
                failure = galleryFailure(context, bugs, "load a gallery image", e)
            }
        }

        val shown = fitted
        val failed = failure
        when {
            failed != null -> {
                // A picture that did not load, or whose tiles core could not give, which the person may try again.
                AndroidNotice(
                    title = unloaded,
                    advice = null,
                    error = failed.error,
                    look = NoticeLook.CENTERED,
                    action = if (failed.offersRetry) failed.error.retryAction { attempt++ } else null,
                )
            }

            shown != null -> {
                ZoomableGalleryImage(
                    bitmap = shown,
                    content = content,
                    contentDescription = title,
                    onFailure = { failure = it },
                )
            }

            else -> {
                GalleryImageLoading()
            }
        }
    }
}

/** The spinner naming the load of a gallery image, on the gallery's backdrop. */
@Composable
private fun GalleryImageLoading() {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.group),
    ) {
        AndroidSpinner(color = BaeTheme.colors.onFill)
        Text(
            text = stringResource(R.string.gallery_loading_image),
            style = ThemeText.body.style,
            color = BaeTheme.colors.onFillSecondary,
        )
    }
}

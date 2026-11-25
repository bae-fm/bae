package fm.bae.app.ui.albumdetail

import android.graphics.Bitmap
import android.util.LruCache
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import fm.bae.app.BaeLogger
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.LocalBugReports
import fm.bae.app.LocalErrorDisplays
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.data.ImageContent
import fm.bae.app.data.LocalPictureTileStore
import fm.bae.app.data.PictureTileStore
import fm.bae.app.data.UnreadableImage
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeCoverImageSource
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePictureTiles
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min

private const val TAG = "bae.GalleryZoom"
private val logger = BaeLogger(TAG)

/** The zoom past which the picture counts as zoomed and draws from tiles. */
private const val ZOOMED = 1.01f

/** What the decoded tiles take in memory at most: about four screens of a large phone. */
private const val TILE_BYTE_BUDGET = 64 * 1024 * 1024

/** One tile of a picture: column [x] and row [y] of level [level], 0 being the picture's own size. */
internal data class TileKey(
    val level: Int,
    val x: Int,
    val y: Int,
)

/** Where a picture's tiles fall on the picture fitted [size] pixels large, before any zoom, over the copy fitted
 *  to it, [copyPixels] wide. */
internal class TileGrid(
    val tiles: BridgePictureTiles,
    val size: Size,
    val copyPixels: Int,
) {
    /** The level drawn [pixelsAcross] pixels wide: the smallest with a pixel for each one drawn, or the picture's
     *  own size when none has. */
    fun level(pixelsAcross: Float): Int {
        var chosen = 0
        tiles.levels.forEachIndexed { index, level ->
            if (level.width.toFloat() >= floor(pixelsAcross)) chosen = index
        }
        return chosen
    }

    /** The levels finer than the fitted copy, coarsest first: the ones whose tiles show more than it does. */
    val drawnLevels: List<Int>
        get() =
            tiles.levels.indices
                .reversed()
                .filter { tiles.levels[it].width.toInt() > copyPixels }

    /** The area [key] covers on the fitted picture. */
    fun rect(key: TileKey): Rect {
        val level = tiles.levels[key.level]
        val edge = tiles.tileEdge.toInt()
        val left = key.x * edge
        val top = key.y * edge
        val width = min(edge, level.width.toInt() - left)
        val height = min(edge, level.height.toInt() - top)
        val across = size.width / level.width.toFloat()
        val down = size.height / level.height.toFloat()
        return Rect(left * across, top * down, (left + width) * across, (top + height) * down)
    }

    /** The tiles of [index] that [area] of the fitted picture touches. */
    fun keys(
        index: Int,
        area: Rect,
    ): List<TileKey> {
        val level = tiles.levels[index]
        val shown = area.intersect(Rect(Offset.Zero, size))
        val edge = tiles.tileEdge.toFloat()
        val across = level.width.toFloat() / size.width / edge
        val down = level.height.toFloat() / size.height / edge
        val lastColumn = level.columns.toInt() - 1
        val lastRow = level.rows.toInt() - 1
        val firstX = min(lastColumn, floor(shown.left * across).toInt())
        val firstY = min(lastRow, floor(shown.top * down).toInt())
        val lastX = min(lastColumn, ceil(shown.right * across).toInt() - 1)
        val lastY = min(lastRow, ceil(shown.bottom * down).toInt() - 1)
        if (shown.isEmpty || firstX > lastX || firstY > lastY) return emptyList()
        return (firstY..lastY).flatMap { y -> (firstX..lastX).map { x -> TileKey(index, x, y) } }
    }
}

/**
 * The tiles of one picture the viewer zooms: core's levels once a zoom asks for them, and the decoded tiles the
 * viewer draws, fetched as they come on screen. Its reads run in [scope], so the viewer leaving stops them, and core
 * stops the cut when no other viewer wants the picture. A read that fails is told to [onFailure] once, and no more
 * are made.
 */
@Stable
internal class GalleryTiles(
    private val source: BridgeCoverImageSource,
    private val store: PictureTileStore,
    private val scope: CoroutineScope,
    private val onFailure: (Exception) -> Unit,
) {
    /** The picture's levels, once they arrived. */
    var levels by mutableStateOf<BridgePictureTiles?>(null)
        private set

    /** Moves with each tile that arrives, so a draw that reads it draws again. */
    var arrived by mutableIntStateOf(0)
        private set

    private var levelsRead: Job? = null
    private val reads = HashMap<TileKey, Job>()

    /** Whether the levels or a tile failed, which the viewer shows and asks again by making tiles of its own. */
    private var hasFailed = false
    private val held =
        object : LruCache<TileKey, ImageBitmap>(TILE_BYTE_BUDGET) {
            override fun sizeOf(
                key: TileKey,
                value: ImageBitmap,
            ): Int = value.width * value.height * 4
        }

    /** Ask core for the picture's levels, which cuts its tiles the first time: once, and not again after a failure. */
    fun readLevels() {
        if (levels != null || levelsRead != null || hasFailed) return
        val read =
            scope.launch(start = CoroutineStart.LAZY) {
                try {
                    levels = store.levels(source)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    fail(e)
                } finally {
                    levelsRead = null
                }
            }
        levelsRead = read
        read.start()
    }

    private fun fail(error: Exception) {
        if (hasFailed) return
        hasFailed = true
        logger.warning("No tiles of $source: $error")
        onFailure(error)
    }

    /** The tile [key], once it arrived and while memory keeps it. */
    fun tile(key: TileKey): ImageBitmap? = held.get(key)

    /** Keep fetching only the tiles in [wanted], and fetch those of them that are missing: the viewer moved, so the
     *  rest no longer show. */
    fun keepOnly(wanted: Set<TileKey>) {
        val left = reads.keys - wanted
        for (key in left) reads.remove(key)?.cancel()
        for (key in wanted) {
            if (held.get(key) != null || key in reads || hasFailed) continue
            // Started once it is in the map, so a read that ends at once takes itself out.
            val read = scope.launch(start = CoroutineStart.LAZY) { read(key) }
            reads[key] = read
            read.start()
        }
    }

    private suspend fun read(key: TileKey) {
        val self = currentCoroutineContext()[Job]
        try {
            val tile: Bitmap = store.tile(source, key.level.toUInt(), key.x.toUInt(), key.y.toUInt())
            held.put(key, tile.asImageBitmap())
            arrived++
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            fail(e)
        } finally {
            // A newer read of the key may hold its place by now.
            if (reads[key] === self) reads.remove(key)
        }
    }

    /** Draw the tiles of [grid] that [area] of the fitted picture touches, [pixelsAcross] pixels wide: the coarser
     *  levels' tiles first, so a tile still on its way shows the best one here, and the wanted level's last. Draws
     *  nothing where the fitted copy shows as much. */
    fun DrawScope.drawTiles(
        grid: TileGrid,
        area: Rect,
        pixelsAcross: Float,
    ) {
        val wanted = grid.level(pixelsAcross)
        for (level in grid.drawnLevels) {
            if (level < wanted) continue
            for (key in grid.keys(level, area)) {
                val tile = tile(key) ?: continue
                val rect = grid.rect(key)
                drawImage(
                    image = tile,
                    dstOffset = IntOffset(rect.left.toInt(), rect.top.toInt()),
                    dstSize = IntSize(rect.width.toInt() + 1, rect.height.toInt() + 1),
                    filterQuality = FilterQuality.Medium,
                )
            }
        }
    }

    /** The tiles of the level [pixelsAcross] needs that [area], and a tile beyond each side, touches. */
    fun wanted(
        grid: TileGrid,
        area: Rect,
        pixelsAcross: Float,
    ): Set<TileKey> {
        val level = grid.level(pixelsAcross)
        if (level !in grid.drawnLevels) return emptySet()
        val margin =
            grid.size.width /
                grid.tiles.levels[level]
                    .width
                    .toFloat() * grid.tiles.tileEdge.toFloat()
        return grid.keys(level, area.inflate(margin)).toSet()
    }
}

/** Where the zoomed picture sits on the page: it is drawn [scale] times its fitted size, moved by [offset]. */
internal data class GalleryZoom(
    val scale: Float,
    val offset: Offset,
) {
    val zoomed: Boolean get() = scale > ZOOMED

    /** The part of the fitted picture, at [fit] on a page [page] large, that the page shows. */
    fun shown(
        page: Size,
        fit: Rect,
    ): Rect =
        Rect(
            -offset.x / scale - fit.left,
            -offset.y / scale - fit.top,
            (page.width - offset.x) / scale - fit.left,
            (page.height - offset.y) / scale - fit.top,
        )

    /** Zoom by [zoom] about the page point [centroid] and move by [pan], kept so the picture covers the page on each
     *  side it overflows and is centred on each side it does not. */
    fun moved(
        zoom: Float,
        centroid: Offset,
        pan: Offset,
        page: Size,
        fit: Rect,
    ): GalleryZoom {
        val newScale = max(1f, scale * zoom)
        val factor = newScale / scale
        val raw = centroid - (centroid - offset) * factor + pan
        return GalleryZoom(
            newScale,
            Offset(
                kept(raw.x, newScale, fit.left, fit.width, page.width),
                kept(raw.y, newScale, fit.top, fit.height, page.height),
            ),
        )
    }

    private fun kept(
        moved: Float,
        scale: Float,
        start: Float,
        length: Float,
        page: Float,
    ): Float =
        if (length * scale <= page) {
            (page - length * scale) / 2 - start * scale
        } else {
            moved.coerceIn(page - (start + length) * scale, -start * scale)
        }

    companion object {
        val Fitted = GalleryZoom(1f, Offset.Zero)
    }
}

/**
 * [bitmap], the copy decoded to fit the page, which the person pinches to zoom and drags to pan while zoomed. Once
 * zoomed past the copy, the picture draws from the tiles core cuts, asking for those on screen at the level the
 * zoom needs. The zoom stays until the person zooms out; [content] starts it again.
 */
@Composable
internal fun ZoomableGalleryImage(
    bitmap: Bitmap,
    content: ImageContent,
    contentDescription: String?,
    onFailure: (GalleryFailure) -> Unit,
) {
    val store = LocalPictureTileStore.current
    val scope = rememberScreenScope()
    val bugs = LocalBugReports.current
    val errors = LocalErrorDisplays.current ?: LocaleErrorDisplays(LocalContext.current)
    val tiles =
        remember(content) {
            content.tiledSource?.let { source ->
                GalleryTiles(source, store, scope) { error ->
                    onFailure(galleryFailure(errors, bugs, "zoom a picture", error))
                }
            }
        }
    val fitted = remember(bitmap) { bitmap.asImageBitmap() }
    var zoom by remember(content) { mutableStateOf(GalleryZoom.Fitted) }

    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val page = Size(constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat())
        val fit = fitted(page, bitmap.width.toFloat() / bitmap.height.toFloat())
        if (tiles != null) {
            ReadTilesOnScreen(tiles, { zoom }, page, fit, bitmap.width)
        }
        ZoomedPicture(
            fitted = fitted,
            tiles = tiles,
            page = page,
            fit = fit,
            copyPixels = bitmap.width,
            zoom = zoom,
            contentDescription = contentDescription,
            modifier = Modifier.zoomGestures(content, page, fit, { zoom }, { zoom = it }),
        )
    }
}

/** The fitted copy, with the tiles drawn over it once the picture is zoomed past it. */
@Composable
private fun ZoomedPicture(
    fitted: ImageBitmap,
    tiles: GalleryTiles?,
    page: Size,
    fit: Rect,
    copyPixels: Int,
    zoom: GalleryZoom,
    contentDescription: String?,
    modifier: Modifier,
) {
    // The canvas covers the fitted copy, as the picture's own node, and draws the zoomed picture past it.
    Canvas(
        modifier =
            modifier
                .fillMaxWidth()
                .aspectRatio(fit.width / fit.height)
                .semantics {
                    role = Role.Image
                    contentDescription?.let { this.contentDescription = it }
                },
    ) {
        // The canvas's own space is the fitted picture's; the zoom is the page's.
        withTransform({
            translate(fit.left * (zoom.scale - 1) + zoom.offset.x, fit.top * (zoom.scale - 1) + zoom.offset.y)
            scale(zoom.scale, zoom.scale, pivot = Offset.Zero)
        }) {
            drawImage(
                image = fitted,
                dstSize = IntSize(size.width.toInt(), size.height.toInt()),
                filterQuality = FilterQuality.Medium,
            )
            val grid = tiles?.levels?.let { TileGrid(it, fit.size, copyPixels) }
            if (tiles != null && grid != null && zoom.zoomed) {
                tiles.arrived
                with(tiles) { drawTiles(grid, zoom.shown(page, fit), fit.width * zoom.scale) }
            }
        }
    }
}

/**
 * Why a gallery picture did not load or zoom, for the notice that says so: what core says when it could not do it
 * ([error], when core said anything to show), and whether trying again can get past it.
 */
internal class GalleryFailure(
    val error: DisplayError?,
    val offersRetry: Boolean,
) {
    companion object {
        /** The store found no bytes for the image, which another read may find. */
        val NoBytes = GalleryFailure(null, offersRetry = true)

        /** A picture file this device cannot decode, which reading again does not mend, or a bug. */
        val Final = GalleryFailure(null, offersRetry = false)
    }
}

/**
 * What stopped a gallery picture from loading or zooming: what core says when it could not do it, whose error says
 * whether trying again can help; a picture file this device cannot decode, which retrying does not mend; and
 * otherwise a bug, which is reported and not retried.
 */
internal fun galleryFailure(
    errors: ErrorDisplays,
    bugs: BugReports,
    operation: String,
    error: Exception,
): GalleryFailure =
    when (error) {
        is BridgeException -> {
            val shown = errors.display(error)
            GalleryFailure(shown, shown?.offersRetry ?: false)
        }

        is UnreadableImage -> {
            GalleryFailure.Final
        }

        else -> {
            bugs.report(operation, error)
            GalleryFailure.Final
        }
    }

/** Where a picture of [aspect] sits fitted on a page [page] large: as large as fits, centred. */
private fun fitted(
    page: Size,
    aspect: Float,
): Rect {
    val width = min(page.width, page.height * aspect)
    val height = width / aspect
    return Rect(Offset((page.width - width) / 2, (page.height - height) / 2), Size(width, height))
}

/** While the picture is zoomed, ask core for its levels and then for the tiles on screen, and stop the reads of the
 *  tiles that leave it. */
@Composable
private fun ReadTilesOnScreen(
    tiles: GalleryTiles,
    zoom: () -> GalleryZoom,
    page: Size,
    fit: Rect,
    copyPixels: Int,
) {
    LaunchedEffect(tiles, page) {
        snapshotFlow { zoom() to tiles.levels }.collect { (shown, levels) ->
            if (!shown.zoomed) return@collect
            tiles.readLevels()
            if (levels == null) return@collect
            val grid = TileGrid(levels, fit.size, copyPixels)
            tiles.keepOnly(tiles.wanted(grid, shown.shown(page, fit), fit.width * shown.scale))
        }
    }
}

/** Pinch to zoom about the fingers, and drag to pan while zoomed. A drag at the fitted size is left to the pager
 *  and the dismiss drag. */
private fun Modifier.zoomGestures(
    content: ImageContent,
    page: Size,
    fit: Rect,
    zoom: () -> GalleryZoom,
    moved: (GalleryZoom) -> Unit,
): Modifier =
    pointerInput(content, page) {
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false)
            do {
                val event = awaitPointerEvent()
                val pinch = event.calculateZoom()
                if (pinch != 1f || zoom().zoomed) {
                    val centroid = event.calculateCentroid() + fit.topLeft
                    moved(zoom().moved(pinch, centroid, event.calculatePan(), page, fit))
                    // Consume so the pager and the dismiss drag don't take a pinch or a pan.
                    event.changes.forEach { it.consume() }
                }
            } while (event.changes.any { it.pressed })
        }
    }

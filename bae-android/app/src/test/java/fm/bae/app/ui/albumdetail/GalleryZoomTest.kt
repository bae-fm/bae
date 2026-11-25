package fm.bae.app.ui.albumdetail

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.data.PictureTileStore
import fm.bae.app.data.TestImages
import fm.bae.app.data.UndecodableTile
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.bae_bridge.BridgeCoverImageSource
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgePictureTiles
import uniffi.bae_bridge.BridgeTileLevel
import java.util.Collections

/** What the gallery's zoom promises: it draws the level the screen's pixels need, keeps the picture over the page, and
 *  reads only the tiles on screen. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class GalleryZoomTest {
    /** A 6000 by 4000 picture's levels, as core cuts them: halving from its own size until one tile holds it. */
    private val levels =
        BridgePictureTiles(
            tileEdge = 512u,
            levels =
                listOf(
                    level(6000u, 4000u),
                    level(3000u, 2000u),
                    level(1500u, 1000u),
                    level(750u, 500u),
                    level(375u, 250u),
                ),
        )

    private fun level(
        width: UInt,
        height: UInt,
    ) = BridgeTileLevel(width = width, height = height, columns = (width + 511u) / 512u, rows = (height + 511u) / 512u)

    private val failures = Collections.synchronizedList(mutableListOf<Exception>())

    /** The picture fitted 1200 by 800 pixels large. */
    private val grid = TileGrid(levels, Size(1200f, 800f), copyPixels = 1200)

    @Test
    fun aZoomDrawsTheSmallestLevelWithAPixelForEachOneDrawn() {
        assertEquals("at rest, 1200 pixels across", 2, grid.level(1200f))
        assertEquals("zoomed twice, 2400 pixels", 1, grid.level(2400f))
        assertEquals("zoomed four times, 4800 pixels", 0, grid.level(4800f))
        assertEquals("further in than the picture's pixels", 0, grid.level(50_000f))
        assertEquals(listOf(2, 1, 0), grid.drawnLevels)
        assertEquals(emptyList<Int>(), TileGrid(levels, grid.size, copyPixels = 6000).drawnLevels)
    }

    @Test
    fun theTilesAnAreaTouchesAndNoneBeyondThePicture() {
        assertEquals(
            (0..2).map { TileKey(0, it, 0) },
            grid.keys(0, Rect(100f, 0f, 220f, 20f)),
        )
        assertEquals(listOf(TileKey(4, 0, 0)), grid.keys(4, Rect(-100f, -100f, 2000f, 2000f)))
        assertTrue(grid.keys(0, Rect(1300f, 0f, 1400f, 10f)).isEmpty())
        val last = grid.rect(TileKey(0, 11, 7))
        assertEquals(1200f, last.right, 0.01f)
        assertEquals(800f, last.bottom, 0.01f)
    }

    @Test
    fun aZoomKeepsThePictureOverThePageAndCentredWhereItFits() {
        // A picture 1200 by 800 fitted on a page 1200 by 2000, so 600 above and below it.
        val page = Size(1200f, 2000f)
        val fit = Rect(Offset(0f, 600f), Size(1200f, 800f))
        val twice = GalleryZoom.Fitted.moved(2f, Offset(600f, 1000f), Offset.Zero, page, fit)
        assertEquals(2f, twice.scale)
        // 1600 high, so still centred on the page's 2000.
        assertEquals(200f - 600f * 2, twice.offset.y, 0.01f)
        // Dragged far right: its left side stays at the page's left.
        val dragged = twice.moved(1f, Offset.Zero, Offset(5000f, 0f), page, fit)
        assertEquals(0f, dragged.offset.x, 0.01f)
        // Pinched out past the fitted size: back to it.
        assertEquals(GalleryZoom.Fitted.scale, dragged.moved(0.1f, Offset.Zero, Offset.Zero, page, fit).scale)
    }

    @Test
    fun aTileThatLeavesTheScreenStopsItsRead() =
        runBlocking {
            val started = Collections.synchronizedList(mutableListOf<TileKey>())
            val stopped = Collections.synchronizedList(mutableListOf<TileKey>())
            val store =
                PictureTileStore(
                    tilesOf { level, x, y ->
                        val key = TileKey(level.toInt(), x.toInt(), y.toInt())
                        started += key
                        try {
                            awaitCancellation()
                        } finally {
                            stopped += key
                        }
                    },
                    Dispatchers.Unconfined,
                )
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val tiles = GalleryTiles(source(), store, scope) { failures += it }
            val kept = TileKey(0, 0, 0)
            val left = TileKey(0, 1, 0)

            tiles.keepOnly(setOf(kept, left))
            assertEquals(listOf(kept, left), started.toList())
            tiles.keepOnly(setOf(kept))
            yield()
            assertEquals(listOf(left), stopped.toList())
            scope.cancel()
            yield()
            assertEquals("the viewer leaving stops every read", listOf(left, kept), stopped.toList())
        }

    @Test
    fun anArrivedTileIsHeldAndNotAskedForAgain() =
        runBlocking {
            val asked = Collections.synchronizedList(mutableListOf<TileKey>())
            val png = TestImages.png(512)
            val store =
                PictureTileStore(
                    tilesOf { level, x, y ->
                        asked += TileKey(level.toInt(), x.toInt(), y.toInt())
                        png
                    },
                    Dispatchers.Unconfined,
                )
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val tiles = GalleryTiles(source(), store, scope) { failures += it }
            val key = TileKey(2, 1, 1)

            tiles.keepOnly(setOf(key))
            assertNotNull(tiles.tile(key))
            assertEquals(1, tiles.arrived)
            tiles.keepOnly(setOf(key))
            assertEquals(listOf(key), asked.toList())
            scope.cancel()
        }

    /** An app handle that answers each tile with [tile]. */
    private fun tilesOf(tile: suspend (level: UInt, x: UInt, y: UInt) -> ByteArray) =
        object : FakeAppHandle() {
            override suspend fun pictureTile(
                source: BridgeCoverImageSource,
                level: UInt,
                x: UInt,
                y: UInt,
            ): ByteArray = tile(level, x, y)
        }

    @Test
    fun aTileThatFailsIsToldOnceAndNoMoreAreAskedFor() =
        runBlocking {
            val asked = Collections.synchronizedList(mutableListOf<TileKey>())
            val store =
                PictureTileStore(
                    tilesOf { level, x, y ->
                        asked += TileKey(level.toInt(), x.toInt(), y.toInt())
                        error("core made no tile")
                    },
                    Dispatchers.Unconfined,
                )
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val tiles = GalleryTiles(source(), store, scope) { failures += it }

            tiles.keepOnly(setOf(TileKey(0, 0, 0), TileKey(0, 1, 0)))
            tiles.keepOnly(setOf(TileKey(0, 2, 0)))
            assertEquals("one failure is told, and no read starts after it", 1, failures.size)
            assertEquals(listOf(TileKey(0, 0, 0)), asked.toList())
            scope.cancel()
        }

    @Test
    fun aTileThatDoesNotDecodeIsAnUndecodableTileAndNotABadPictureFile() =
        runBlocking {
            val store = PictureTileStore(tilesOf { _, _, _ -> byteArrayOf(1, 2, 3) }, Dispatchers.Unconfined)

            val thrown = runCatching { store.tile(source(), 0u, 0u, 0u) }.exceptionOrNull()

            assertTrue("$thrown", thrown is UndecodableTile)
        }

    private fun source() =
        BridgeCoverImageSource.ReleaseImage(
            releaseId = "release-1",
            source = BridgeGallerySource.ReleaseFile(fileId = "scan-1"),
        )
}

package fm.bae.app.data

import android.graphics.Bitmap
import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeCoverImageSource

/**
 * The tiles of the pictures a viewer zooms: their levels, and each tile decoded at its own size on a bounded
 * dispatcher. Unlike [ImageStore], it caches nothing: a viewer holds the few tiles it draws.
 */
class PictureTileStore(
    private val handle: AppHandleInterface,
    /** Where reads and decodes run. Injected so tests drive them on their own scheduler. */
    private val dispatcher: CoroutineDispatcher = boundedDecodeDispatcher(),
) {
    /**
     * The levels of the tiles a viewer zooms [source] from, 0 being the picture's own size. Core cuts the picture
     * into tiles the first time, and stops the cut when the call is cancelled and no other call wants the picture.
     */
    suspend fun levels(source: BridgeCoverImageSource) = withContext(dispatcher) { handle.pictureTiles(source) }

    /** One tile of [source]: column [x] and row [y] of level [level], decoded at its own size. */
    suspend fun tile(
        source: BridgeCoverImageSource,
        level: UInt,
        x: UInt,
        y: UInt,
    ): Bitmap =
        withContext(dispatcher) {
            val bytes = handle.pictureTile(source, level, x, y)
            decodeBytes(bytes, DecodeSize.Native) ?: throw UndecodableTile(level, x, y, bytes.size)
        }
}

/** A tile that core cut itself and this device cannot decode, which is a bug and not a bad picture file. */
class UndecodableTile(
    level: UInt,
    x: UInt,
    y: UInt,
    byteCount: Int,
) : Exception("tile $level/${x}_$y: $byteCount bytes that aren't an image this device decodes")

/**
 * The tile store for the open library, provided once at the composition root beside [LocalImageStore]. No default:
 * zooming a picture outside that provider is a wiring bug, so it fails loudly.
 */
val LocalPictureTileStore =
    staticCompositionLocalOf<PictureTileStore> {
        error("LocalPictureTileStore not provided")
    }

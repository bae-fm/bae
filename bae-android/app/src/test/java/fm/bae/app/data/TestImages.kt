package fm.bae.app.data

import android.graphics.Bitmap
import java.io.ByteArrayOutputStream
import kotlin.random.Random

/**
 * Encoded images for the image-store tests. Pixels are pseudo-random from a fixed
 * seed so an encoded image is both reproducible and incompressible — a solid fill
 * would collapse to a few hundred bytes, which says nothing about a cache bounded
 * in bytes.
 */
object TestImages {
    fun png(
        width: Int,
        height: Int = width,
        seed: Int = 1,
    ): ByteArray = encode(noise(width, height, seed), Bitmap.CompressFormat.PNG)

    fun jpeg(
        width: Int,
        height: Int = width,
        seed: Int = 1,
    ): ByteArray = encode(noise(width, height, seed), Bitmap.CompressFormat.JPEG)

    /**
     * A square whose quadrants are flat [colors] — top left, top right, bottom
     * left, bottom right — so a test can say where a corner of the source ended
     * up. Flat blocks survive JPEG's chroma subsampling away from their edges,
     * which noise would not.
     */
    fun quadrantsJpeg(
        size: Int,
        colors: List<Int>,
    ): ByteArray = encode(quadrants(size, colors), Bitmap.CompressFormat.JPEG)

    private fun noise(
        width: Int,
        height: Int,
        seed: Int,
    ): Bitmap {
        val random = Random(seed)
        val pixels = IntArray(width * height) { random.nextInt() or 0xFF000000.toInt() }
        return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
    }

    private fun quadrants(
        size: Int,
        colors: List<Int>,
    ): Bitmap {
        val half = size / 2
        val pixels =
            IntArray(size * size) { index ->
                val x = index % size
                val y = index / size
                colors[(if (y < half) 0 else 2) + (if (x < half) 0 else 1)]
            }
        return Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
    }

    private fun encode(
        bitmap: Bitmap,
        format: Bitmap.CompressFormat,
    ): ByteArray =
        ByteArrayOutputStream().use { out ->
            check(bitmap.compress(format, 100, out)) { "could not encode a $format test image" }
            out.toByteArray()
        }
}

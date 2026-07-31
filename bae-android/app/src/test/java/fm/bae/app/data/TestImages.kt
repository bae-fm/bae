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

    private fun noise(
        width: Int,
        height: Int,
        seed: Int,
    ): Bitmap {
        val random = Random(seed)
        val pixels = IntArray(width * height) { random.nextInt() or 0xFF000000.toInt() }
        return Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888)
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

package fm.bae.app.data

import android.graphics.Bitmap
import android.graphics.Color
import androidx.core.graphics.ColorUtils
import java.io.ByteArrayOutputStream
import kotlin.random.Random

/**
 * Encoded images for the image-store tests. Pixels are pseudo-random from a fixed
 * seed so an encoded image is both reproducible and incompressible — a solid fill
 * would collapse to a few hundred bytes, which says nothing about a cache bounded
 * in bytes.
 */
object TestImages {
    private const val GRADIENT_HUES = 12

    fun png(
        width: Int,
        height: Int = width,
        seed: Int = 1,
    ): ByteArray = encode(noise(width, height, seed), Bitmap.CompressFormat.PNG)

    /**
     * A cover as the boards draw one: a diagonal fade from a light colour at the top left to a dark one at the
     * bottom right, both from a hue that [seed] picks.
     */
    fun gradientPng(
        size: Int,
        seed: Int,
    ): ByteArray = encode(gradient(size, seed), Bitmap.CompressFormat.PNG)

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

    private fun gradient(
        size: Int,
        seed: Int,
    ): Bitmap {
        val hue = Math.floorMod(seed, GRADIENT_HUES) * (360f / GRADIENT_HUES)
        val light = Color.HSVToColor(floatArrayOf(hue, 0.55f, 0.70f))
        val dark = Color.HSVToColor(floatArrayOf((hue + 40f) % 360f, 0.70f, 0.35f))
        val pixels =
            IntArray(size * size) { index ->
                val fade = ((index % size) + (index / size)) / (2f * (size - 1))
                ColorUtils.blendARGB(light, dark, fade)
            }
        return Bitmap.createBitmap(pixels, size, size, Bitmap.Config.ARGB_8888)
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

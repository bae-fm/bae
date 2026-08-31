package fm.bae.app.data

import androidx.exifinterface.media.ExifInterface
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The decode itself: how much of a source it reads, and that it lands upright.
 *
 * Native graphics so these run the platform decoder rather than a shadow that
 * reports whatever dimensions it is asked for.
 */
private const val RED = 0xFFFF0000.toInt()
private const val GREEN = 0xFF00FF00.toInt()
private const val BLUE = 0xFF0000FF.toInt()
private const val WHITE = 0xFFFFFFFF.toInt()

/** The quadrant-coloured source the orientation cases are decoded from: red top
 *  left, green top right, blue bottom left, white bottom right. */
private const val QUADRANT_SOURCE = 64
private val SOURCE_QUADRANTS = listOf(RED, GREEN, BLUE, WHITE)

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ImageDecodeTest {
    @get:Rule
    val folder = TemporaryFolder()

    @Test
    fun downsamplesToTheSmallestReductionThatStillFillsTheSlot() {
        // The reduction is a power of two, so a 1000px source asked for 100px
        // decodes at 125 — the next step down (62) would no longer fill the slot.
        assertEquals(8, sampleSize(width = 1000, height = 1000, size = DecodeSize.FitTo(100)))
        assertEquals(1, sampleSize(width = 100, height = 100, size = DecodeSize.FitTo(100)))
        assertEquals(1, sampleSize(width = 64, height = 64, size = DecodeSize.FitTo(100)))
        // The longer edge decides, so a wide slot still gets enough pixels.
        assertEquals(2, sampleSize(width = 400, height = 100, size = DecodeSize.FitTo(200)))
    }

    @Test
    fun readsTheWholeSourceWhenNoSizeIsAsked() {
        assertEquals(1, sampleSize(width = 4000, height = 4000, size = DecodeSize.Native))
        // A slot measured before layout gave it bounds asks for nothing; reading
        // the source whole is the only answer that isn't a guess.
        assertEquals(1, sampleSize(width = 4000, height = 4000, size = DecodeSize.FitTo(0)))
    }

    @Test
    fun decodesBytesAtTheAskedSize() {
        val bytes = TestImages.png(512)

        assertEquals(64, decodeBytes(bytes, DecodeSize.FitTo(64))!!.width)
        assertEquals(512, decodeBytes(bytes, DecodeSize.Native)!!.width)
    }

    @Test
    fun answersNothingForBytesThatArenTAnImage() {
        assertNull(decodeBytes("not an image".toByteArray(), DecodeSize.FitTo(64)))
    }

    @Test
    fun standsAQuarterTurnedSourceUpright() {
        // A scan whose pixels are stored sideways with the orientation tag saying
        // so: decoded as-is it would be 200 × 100, and upright it is 100 × 200.
        val file = folder.newFile("sideways.jpg")
        file.writeBytes(TestImages.jpeg(width = 200, height = 100))
        ExifInterface(file.path).apply {
            setAttribute(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_ROTATE_90.toString())
            saveAttributes()
        }

        val decoded = decodeBytes(file.readBytes(), DecodeSize.Native)!!
        assertEquals(100, decoded.width)
        assertEquals(200, decoded.height)
    }

    @Test
    fun standsAMirroredOrTurnedSourceUpright() {
        // Dimensions can't tell a mirrored source from an upright one, so this
        // follows the four quadrants instead: each orientation names where the
        // source's own corners belong once it stands upright.
        assertUpright(ExifInterface.ORIENTATION_ROTATE_90, listOf(BLUE, RED, WHITE, GREEN))
        assertUpright(ExifInterface.ORIENTATION_ROTATE_180, listOf(WHITE, BLUE, GREEN, RED))
        assertUpright(ExifInterface.ORIENTATION_ROTATE_270, listOf(GREEN, WHITE, RED, BLUE))
        assertUpright(ExifInterface.ORIENTATION_FLIP_HORIZONTAL, listOf(GREEN, RED, WHITE, BLUE))
        assertUpright(ExifInterface.ORIENTATION_FLIP_VERTICAL, listOf(BLUE, WHITE, RED, GREEN))
        // The two diagonal reflections: transpose holds the main diagonal's
        // corners in place, transverse the anti-diagonal's.
        assertUpright(ExifInterface.ORIENTATION_TRANSPOSE, listOf(RED, BLUE, GREEN, WHITE))
        assertUpright(ExifInterface.ORIENTATION_TRANSVERSE, listOf(WHITE, GREEN, BLUE, RED))
    }

    /** Decode a quadrant-coloured source tagged [orientation] and assert its
     *  quadrants land in [expected] order: top left, top right, bottom left,
     *  bottom right. */
    private fun assertUpright(
        orientation: Int,
        expected: List<Int>,
    ) {
        val file = folder.newFile("orientation-$orientation.jpg")
        file.writeBytes(TestImages.quadrantsJpeg(QUADRANT_SOURCE, SOURCE_QUADRANTS))
        ExifInterface(file.path).apply {
            setAttribute(ExifInterface.TAG_ORIENTATION, orientation.toString())
            saveAttributes()
        }

        val decoded = decodeBytes(file.readBytes(), DecodeSize.Native)!!
        val near = QUADRANT_SOURCE / 4
        val far = QUADRANT_SOURCE - near
        val quadrants =
            listOf(
                decoded.getPixel(near, near),
                decoded.getPixel(far, near),
                decoded.getPixel(near, far),
                decoded.getPixel(far, far),
            )
        quadrants.forEachIndexed { index, actual ->
            assertEquals("quadrant $index of orientation $orientation", hue(expected[index]), hue(actual))
        }
    }

    /** Which of the source's four colours a decoded pixel is. JPEG shifts a flat
     *  block's exact value, so compare the channels that are on rather than their
     *  levels. */
    private fun hue(color: Int): String =
        listOf(16, 8, 0).joinToString("") { shift ->
            if ((color shr shift) and 0xFF > 0x80) "1" else "0"
        }

    @Test
    fun leavesAnUprightSourceAlone() {
        val file = folder.newFile("upright.jpg")
        file.writeBytes(TestImages.jpeg(width = 200, height = 100))

        val decoded = decodeBytes(file.readBytes(), DecodeSize.Native)!!
        assertEquals(200, decoded.width)
        assertEquals(100, decoded.height)
    }
}

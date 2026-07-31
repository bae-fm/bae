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
    fun decodesAFileAtTheAskedSize() {
        val file = folder.newFile("art.png")
        file.writeBytes(TestImages.png(512))

        assertEquals(64, decodeFile(file.path, DecodeSize.FitTo(64))!!.width)
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

        val decoded = decodeFile(file.path, DecodeSize.Native)!!
        assertEquals(100, decoded.width)
        assertEquals(200, decoded.height)
    }

    @Test
    fun leavesAnUprightSourceAlone() {
        val file = folder.newFile("upright.jpg")
        file.writeBytes(TestImages.jpeg(width = 200, height = 100))

        val decoded = decodeFile(file.path, DecodeSize.Native)!!
        assertEquals(200, decoded.width)
        assertEquals(100, decoded.height)
    }
}

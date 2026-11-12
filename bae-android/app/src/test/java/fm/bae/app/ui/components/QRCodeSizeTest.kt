package fm.bae.app.ui.components

import android.app.Application
import fm.bae.app.BaeRobolectricTestRunner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.Random

/**
 * A QR code is drawn as wide as its payload needs, never at a side the caller
 * picked: the Add device and invite codes carry the S3 key, and a camera cannot
 * read their modules when they are drawn smaller than the measured minimum.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class QRCodeSizeTest {
    private fun side(text: String): Float {
        val bitmap = encodeQRCode(text)
        assertNotNull(bitmap)
        return qrSide(bitmap!!).value
    }

    @Test
    fun anInviteCarryingAnS3KeyIsDrawnAtLeastTheDocumentedSize() {
        val bitmap = encodeQRCode(DenseCodes.inviteWithKey(Random(2)))!!
        assertEquals(bitmap.width * 360f / 105f, qrSide(bitmap).value, 0.01f)
        assertTrue("drawn ${qrSide(bitmap)}", qrSide(bitmap).value >= 250f)
    }

    @Test
    fun aRestoreCodeCarryingAnS3KeyIsDrawnAtLeastItsModulesAtTheMinimum() {
        val bitmap = encodeQRCode(DenseCodes.restoreCodeWithKey(Random(1)))!!
        assertTrue(qrSide(bitmap).value >= bitmap.width * 360f / 105f - 0.01f)
        assertTrue(qrSide(bitmap).value >= 220f)
    }

    @Test
    fun aShortPairingCodeStaysSmall() {
        assertTrue(side("PAIR-1234-5678") < 120f)
    }
}

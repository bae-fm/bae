package fm.bae.app.ui.components

import android.graphics.ImageFormat
import android.graphics.Rect
import android.media.Image
import androidx.camera.core.ImageInfo
import androidx.camera.core.ImageProxy
import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer
import java.util.Random

class QrFrameDecoderTest {
    @Test
    fun successfulFrameScansAndCloses() {
        val imageProxy = qrImageProxy("qr-text")

        val outcome = QrFrameDecoder().analyze(imageProxy)

        assertEquals(QrFrameOutcome.Found("qr-text"), outcome)
        assertTrue(imageProxy.closed)
    }

    @Test
    fun missingCodeFrameClosesWithoutFailure() {
        val imageProxy = blankImageProxy(width = 128, height = 128, rowStride = 128)

        val outcome = QrFrameDecoder().analyze(imageProxy)

        assertEquals(QrFrameOutcome.NotFound, outcome)
        assertTrue(imageProxy.closed)
    }

    @Test
    fun malformedFrameReportsFailureAndCloses() {
        val imageProxy = blankImageProxy(width = 128, height = 128, rowStride = 64)

        val outcome = QrFrameDecoder().analyze(imageProxy)

        assertTrue((outcome as QrFrameOutcome.Failed).error is IllegalArgumentException)
        assertTrue(imageProxy.closed)
    }

    @Test
    fun aRestoreCodeWithItsS3KeyReadsFromArmsLength() {
        assertReadsFromArmsLength(DenseCodes.restoreCodeWithKey(Random(1)))
    }

    @Test
    fun anInviteWithItsS3KeyReadsFromArmsLength() {
        assertReadsFromArmsLength(DenseCodes.inviteWithKey(Random(2)))
    }

    // Packed, a code with its S3 key is a QR code of version 12 or 13, whose
    // modules a phone still resolves held back at 40 or 45 cm.
    @Test
    fun aRestoreCodeWithItsS3KeyReadsFromFartherBack() {
        assertReadsFrom(DenseCodes.restoreCodeWithKey(Random(1)), FARTHER_BACK_CM)
    }

    @Test
    fun anInviteWithItsS3KeyReadsFromFartherBack() {
        assertReadsFrom(DenseCodes.inviteWithKey(Random(2)), FARTHER_BACK_CM)
    }

    private fun assertReadsFromArmsLength(text: String) = assertReadsFrom(text, ARMS_LENGTH_CM)

    // A phone held [distancesCm] from the Mac's screen, at a few tilts, frames
    // the code as the scanner's camera delivers it. A live scan analyses
    // several frames a second, so most of them reading is a scan that lands.
    private fun assertReadsFrom(
        text: String,
        distancesCm: List<Double>,
    ) {
        val decoder = QrFrameDecoder()
        for (distanceCm in distancesCm) {
            val read =
                TILTS_DEGREES.count { tilt ->
                    val luma = MacQrScene(MAC_DISPLAY, distanceCm, tilt).luma(text, QR_ANALYSIS_WIDTH, QR_ANALYSIS_HEIGHT, ROW_STRIDE)
                    val frame = FakeImageProxy(QR_ANALYSIS_WIDTH, QR_ANALYSIS_HEIGHT, ROW_STRIDE, luma)
                    decoder.analyze(frame) == QrFrameOutcome.Found(text)
                }
            assertTrue(
                "${text.length}-char code read in $read of ${TILTS_DEGREES.size} frames from $distanceCm cm",
                read >= TILTS_DEGREES.size * 2 / 3,
            )
        }
    }

    private companion object {
        /** As QRCode.swift draws a code. */
        val MAC_DISPLAY = MacQrDisplay(sidePoints = 360.0, quietZoneModules = 4, correction = ErrorCorrectionLevel.L)
        val ARMS_LENGTH_CM = listOf(25.0, 30.0)
        val FARTHER_BACK_CM = listOf(40.0, 45.0)
        val TILTS_DEGREES = listOf(0.0, 2.0, 4.0, 7.0, 11.0, 17.0)

        // Camera rows are padded past the frame's width.
        const val ROW_STRIDE = QR_ANALYSIS_WIDTH + 64
    }
}

private fun qrImageProxy(text: String): FakeImageProxy {
    val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 128, 128)
    val data =
        ByteArray(matrix.width * matrix.height) { index ->
            val x = index % matrix.width
            val y = index / matrix.width
            if (matrix[x, y]) 0.toByte() else 255.toByte()
        }
    return FakeImageProxy(
        width = matrix.width,
        height = matrix.height,
        rowStrideBytes = matrix.width,
        data = data,
    )
}

private fun blankImageProxy(
    width: Int,
    height: Int,
    rowStride: Int,
): FakeImageProxy =
    FakeImageProxy(
        width = width,
        height = height,
        rowStrideBytes = rowStride,
        data = ByteArray(rowStride * height) { 255.toByte() },
    )

private class FakeImageProxy(
    private val width: Int,
    private val height: Int,
    private val rowStrideBytes: Int,
    private val data: ByteArray,
) : ImageProxy {
    var closed = false
        private set

    private var cropRect = Rect(0, 0, width, height)

    override fun close() {
        closed = true
    }

    override fun getCropRect(): Rect = cropRect

    override fun setCropRect(rect: Rect?) {
        cropRect = rect ?: Rect(0, 0, width, height)
    }

    override fun getFormat(): Int = ImageFormat.YUV_420_888

    override fun getHeight(): Int = height

    override fun getWidth(): Int = width

    override fun getPlanes(): Array<ImageProxy.PlaneProxy> =
        arrayOf(
            object : ImageProxy.PlaneProxy {
                override fun getRowStride(): Int = rowStrideBytes

                override fun getPixelStride(): Int = 1

                override fun getBuffer(): ByteBuffer = ByteBuffer.wrap(data)
            },
        )

    override fun getImageInfo(): ImageInfo = throw UnsupportedOperationException("Image info is not used by QR analysis")

    override fun getImage(): Image? = null
}

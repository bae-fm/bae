package fm.bae.app.ui.components

import android.graphics.ImageFormat
import android.graphics.Rect
import android.media.Image
import androidx.camera.core.ImageInfo
import androidx.camera.core.ImageProxy
import com.google.zxing.BarcodeFormat
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.ByteBuffer

class QRScannerScreenTest {
    @Test
    fun successfulFrameScansAndCloses() {
        val scanned = mutableListOf<String>()
        val imageProxy = qrImageProxy("qr-text")
        var failure: Exception? = null

        analyzeQrFrame(
            reader = qrReader(),
            imageProxy = imageProxy,
            onScanned = { scanned += it },
            onDecodeFailure = { failure = it },
        )

        assertEquals(listOf("qr-text"), scanned)
        assertNull(failure)
        assertTrue(imageProxy.closed)
    }

    @Test
    fun missingCodeFrameClosesWithoutLoggingFailure() {
        val imageProxy = blankImageProxy(width = 128, height = 128, rowStride = 128)
        var scanned = false
        var failure: Exception? = null

        analyzeQrFrame(
            reader = qrReader(),
            imageProxy = imageProxy,
            onScanned = { scanned = true },
            onDecodeFailure = { failure = it },
        )

        assertFalse(scanned)
        assertNull(failure)
        assertTrue(imageProxy.closed)
    }

    @Test
    fun malformedFrameReportsFailureAndCloses() {
        val imageProxy = blankImageProxy(width = 128, height = 128, rowStride = 64)
        var scanned = false
        var failure: Exception? = null

        analyzeQrFrame(
            reader = qrReader(),
            imageProxy = imageProxy,
            onScanned = { scanned = true },
            onDecodeFailure = { failure = it },
        )

        assertFalse(scanned)
        assertTrue(failure is IllegalArgumentException)
        assertTrue(imageProxy.closed)
    }
}

private fun qrReader(): MultiFormatReader =
    MultiFormatReader().apply {
        setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
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

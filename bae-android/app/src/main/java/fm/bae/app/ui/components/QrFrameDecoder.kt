package fm.bae.app.ui.components

import androidx.camera.core.ImageProxy
import com.google.zxing.BinaryBitmap
import com.google.zxing.ChecksumException
import com.google.zxing.DecodeHintType
import com.google.zxing.FormatException
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader

/**
 * The camera frame size the scanner asks for, in the sensor's landscape
 * orientation. A join or invite code with an S3 location and key is QR
 * version 18 to 20 as the Mac draws it; at 1280x720 its modules fall under two
 * camera pixels from a phone held 25 cm from the screen, and ZXing reads
 * nothing.
 */
internal const val QR_ANALYSIS_WIDTH = 2560
internal const val QR_ANALYSIS_HEIGHT = 1440

/** What one analysis frame came to. */
internal sealed interface QrFrameOutcome {
    data class Found(
        val text: String,
    ) : QrFrameOutcome

    /** ZXing found no code in the frame; scanning continues. */
    data object NotFound : QrFrameOutcome

    /**
     * ZXing found a code but its modules failed the format or error-correction
     * checks, as a blurred or too-distant frame does; scanning continues.
     */
    data object Unreadable : QrFrameOutcome

    data class Failed(
        val error: Exception,
    ) : QrFrameOutcome
}

/**
 * Reads a QR code from camera frames. It keeps one copy of the luma plane
 * across frames, so it is confined to the analyzer's thread.
 */
internal class QrFrameDecoder {
    private val reader = QRCodeReader()
    private var luma = ByteArray(0)

    /** Decodes one frame and closes it. */
    fun analyze(imageProxy: ImageProxy): QrFrameOutcome =
        try {
            QrFrameOutcome.Found(decode(imageProxy))
        } catch (_: NotFoundException) {
            QrFrameOutcome.NotFound
        } catch (_: ChecksumException) {
            QrFrameOutcome.Unreadable
        } catch (_: FormatException) {
            QrFrameOutcome.Unreadable
        } catch (e: Exception) {
            QrFrameOutcome.Failed(e)
        } finally {
            imageProxy.close()
        }

    // A YUV_420_888 luma plane has a pixel stride of 1, so its rows are the
    // grayscale image ZXing reads, `rowStride` bytes apart.
    private fun decode(imageProxy: ImageProxy): String {
        val plane = imageProxy.planes[0]
        val buffer = plane.buffer
        val size = buffer.remaining()
        val needed = (imageProxy.height - 1) * plane.rowStride + imageProxy.width
        require(size >= needed) {
            "luma plane holds $size bytes; ${imageProxy.width}x${imageProxy.height} " +
                "at row stride ${plane.rowStride} needs $needed"
        }
        if (luma.size < size) luma = ByteArray(size)
        buffer.get(luma, 0, size)
        val source =
            PlanarYUVLuminanceSource(
                luma,
                plane.rowStride,
                imageProxy.height,
                0,
                0,
                imageProxy.width,
                imageProxy.height,
                false,
            )
        return reader.decode(BinaryBitmap(HybridBinarizer(source)), HINTS).text
    }

    private companion object {
        // With TRY_HARDER the reader looks for finder patterns on every third
        // row; without it, on every (3 * height / 388)th, every eleventh row
        // of a 1440-row frame.
        val HINTS = mapOf(DecodeHintType.TRY_HARDER to true)
    }
}

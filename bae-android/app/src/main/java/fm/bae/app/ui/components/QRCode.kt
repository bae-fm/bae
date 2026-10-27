package fm.bae.app.ui.components

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.tooling.preview.Preview
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import fm.bae.app.BaeLogger
import fm.bae.app.ui.BaeTheme

private const val TAG = "bae.QRCode"
private val logger = BaeLogger(TAG)

/** The quiet zone the QR standard requires, in modules. */
private const val QUIET_ZONE_MODULES = 4

/**
 * [text] as a QR code as wide as the space it is given, or nothing when ZXing
 * rejects the payload; callers always show the copyable text alongside.
 *
 * A join or invite code carrying an S3 location and key reaches QR version
 * 20, so the code takes the full width at the low error-correction level,
 * whose fewer, larger modules a Mac's camera resolves from farther away. The
 * code's base32 part is encoded as alphanumeric rather than bytes, which keeps
 * a restore code beside its S3 key up to three versions smaller.
 */
@Composable
fun QRCodeImage(
    text: String,
    contentDescription: String?,
    modifier: Modifier = Modifier,
) {
    val bitmap = remember(text) { encodeQRCode(text) }
    if (bitmap == null) {
        logger.warning("No QR bitmap to render; the code text is shown alongside")
        return
    }
    // One pixel per module, scaled up without filtering so module edges stay sharp.
    Image(
        painter = BitmapPainter(bitmap.asImageBitmap(), filterQuality = FilterQuality.None),
        contentDescription = contentDescription,
        contentScale = ContentScale.Fit,
        modifier = modifier.fillMaxWidth().aspectRatio(1f),
    )
}

private fun encodeQRCode(text: String): Bitmap? =
    try {
        val hints =
            mapOf(
                EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.L,
                EncodeHintType.QR_COMPACT to true,
                EncodeHintType.MARGIN to QUIET_ZONE_MODULES,
            )
        // A requested size of 1 leaves the matrix at one pixel per module.
        val matrix = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, 1, 1, hints)
        val modules = matrix.width - 2 * QUIET_ZONE_MODULES
        logger.info(
            "QR for a ${text.toByteArray().size}-byte payload: " +
                "version ${(modules - QR_BASE_MODULES) / MODULES_PER_VERSION}, $modules modules, correction L",
        )
        matrix.toBitmap()
    } catch (e: Exception) {
        logger.error("Failed to encode QR code", e)
        null
    }

// A version-n QR code is 17 + 4n modules wide.
private const val QR_BASE_MODULES = 17
private const val MODULES_PER_VERSION = 4

private fun BitMatrix.toBitmap(): Bitmap {
    val pixels = IntArray(width * height)
    for (y in 0 until height) {
        val row = y * width
        for (x in 0 until width) {
            pixels[row + x] = if (this[x, y]) Color.BLACK else Color.WHITE
        }
    }
    return Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).apply {
        setPixels(pixels, 0, width, 0, 0, width, height)
    }
}

@Preview(showBackground = true)
@Composable
private fun QRCodeImagePreview() {
    BaeTheme {
        QRCodeImage(
            text = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
            contentDescription = null,
        )
    }
}

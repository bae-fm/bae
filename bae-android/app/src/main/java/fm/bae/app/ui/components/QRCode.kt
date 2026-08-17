package fm.bae.app.ui.components

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.MultiFormatWriter
import com.google.zxing.common.BitMatrix
import fm.bae.app.BaeLogger
import fm.bae.app.ui.BaeTheme

private const val TAG = "bae.QRCode"
private val logger = BaeLogger(TAG)

/** Side length in pixels of the generated QR bitmap. */
private const val QR_SIZE_PX = 512

/**
 * Render [text] as a QR code square. The same image is shown wherever a device
 * presents its pairing offer for a joining device's camera. Returns
 * null only if ZXing rejects the payload (e.g. too long to encode), which the
 * caller renders around — the copyable text is always shown alongside.
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
    Image(
        painter = BitmapPainter(bitmap.asImageBitmap()),
        contentDescription = contentDescription,
        modifier = modifier,
    )
}

private fun encodeQRCode(text: String): Bitmap? =
    try {
        val matrix = MultiFormatWriter().encode(text, BarcodeFormat.QR_CODE, QR_SIZE_PX, QR_SIZE_PX)
        matrix.toBitmap()
    } catch (e: Exception) {
        logger.error("Failed to encode QR code", e)
        null
    }

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
            text = "bae://pair/placeholder-code",
            contentDescription = null,
            modifier = Modifier.size(200.dp),
        )
    }
}

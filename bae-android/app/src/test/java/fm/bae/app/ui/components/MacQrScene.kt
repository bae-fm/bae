package fm.bae.app.ui.components

import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import com.google.zxing.qrcode.encoder.Encoder
import java.util.Base64
import java.util.Random
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.tan

/**
 * A QR code on a Mac's screen as a phone's rear camera frames it: the code
 * drawn the way the macOS app draws it, at its size on a laptop screen, seen
 * from [distanceCm] at [tiltDegrees], blurred and noisy as a camera frame is.
 */
internal class MacQrScene(
    private val display: MacQrDisplay,
    private val distanceCm: Double,
    private val tiltDegrees: Double,
) {
    /** The luma plane of a [width]x[height] frame whose rows are [rowStride] bytes apart. */
    fun luma(
        text: String,
        width: Int,
        height: Int,
        rowStride: Int,
    ): ByteArray {
        val modules = display.modules(text)
        val pixelsPerCm = width / (2 * distanceCm * tan(Math.toRadians(CAMERA_LONG_SIDE_FOV_DEGREES / 2)))
        val sidePixels = display.sidePoints / SCREEN_POINTS_PER_INCH * CM_PER_INCH * pixelsPerCm
        val pixelsPerModule = sidePixels / modules.size
        val tilt = Math.toRadians(tiltDegrees)
        val image = FloatArray(width * height)
        for (y in 0 until height) {
            for (x in 0 until width) {
                var sum = 0f
                for (sample in 0 until SUBSAMPLES * SUBSAMPLES) {
                    // Offset from the frame's centre, turned into the code's own axes.
                    val dx = x + (sample % SUBSAMPLES + 0.5) / SUBSAMPLES - width / 2.0
                    val dy = y + (sample / SUBSAMPLES + 0.5) / SUBSAMPLES - height / 2.0
                    val u = (cos(tilt) * dx + sin(tilt) * dy) / pixelsPerModule + modules.size / 2.0
                    val v = (-sin(tilt) * dx + cos(tilt) * dy) / pixelsPerModule + modules.size / 2.0
                    val inside = u >= 0 && v >= 0 && u < modules.size && v < modules.size
                    sum +=
                        when {
                            !inside -> DARK_MODE_WINDOW
                            modules[v.toInt()][u.toInt()] -> SCREEN_BLACK
                            else -> SCREEN_WHITE
                        }
                }
                image[y * width + x] = sum / (SUBSAMPLES * SUBSAMPLES)
            }
        }
        val blurred = blur(image, width, height)
        val noise = Random(NOISE_SEED)
        val luma = ByteArray(rowStride * height)
        for (y in 0 until height) {
            for (x in 0 until width) {
                val value = blurred[y * width + x] + noise.nextInt(2 * NOISE + 1) - NOISE
                luma[y * rowStride + x] = value.roundToInt().coerceIn(0, MAX_LUMA).toByte()
            }
        }
        return luma
    }

    private fun blur(
        image: FloatArray,
        width: Int,
        height: Int,
    ): FloatArray {
        val kernel =
            FloatArray(2 * BLUR_RADIUS + 1) { i ->
                val d = (i - BLUR_RADIUS).toDouble()
                exp(-d * d / (2 * BLUR_SIGMA * BLUR_SIGMA)).toFloat()
            }
        val total = kernel.sum()
        val across = FloatArray(width * height)
        for (y in 0 until height) {
            for (x in 0 until width) {
                var sum = 0f
                for (i in kernel.indices) sum += kernel[i] * image[y * width + (x + i - BLUR_RADIUS).coerceIn(0, width - 1)]
                across[y * width + x] = sum / total
            }
        }
        val down = FloatArray(width * height)
        for (y in 0 until height) {
            for (x in 0 until width) {
                var sum = 0f
                for (i in kernel.indices) sum += kernel[i] * across[(y + i - BLUR_RADIUS).coerceIn(0, height - 1) * width + x]
                down[y * width + x] = sum / total
            }
        }
        return down
    }

    private companion object {
        /** A 14-inch MacBook Pro at its default scaled resolution. */
        const val SCREEN_POINTS_PER_INCH = 127.0
        const val CM_PER_INCH = 2.54

        /** A phone main camera of about 26 mm equivalent focal length, across the frame's long side. */
        const val CAMERA_LONG_SIDE_FOV_DEGREES = 67.0

        // Camera luma of the screen's black and white, and of a dark-mode
        // window around the code.
        const val SCREEN_BLACK = 35f
        const val SCREEN_WHITE = 215f
        const val DARK_MODE_WINDOW = 50f
        const val MAX_LUMA = 255

        const val SUBSAMPLES = 2
        const val BLUR_SIGMA = 0.9
        const val BLUR_RADIUS = 3
        const val NOISE = 6
        const val NOISE_SEED = 3L
    }
}

/** How the macOS app draws a code: its side on screen, quiet zone and error correction. */
internal data class MacQrDisplay(
    val sidePoints: Double,
    val quietZoneModules: Int,
    val correction: ErrorCorrectionLevel,
) {
    /**
     * The code's modules with its quiet zone, dark as true. ZXing's compact
     * encoding mixes alphanumeric and byte segments as Core Image's generator
     * does, so it reaches the same QR version.
     */
    fun modules(text: String): Array<BooleanArray> {
        val symbol = Encoder.encode(text, correction, mapOf(EncodeHintType.QR_COMPACT to true)).matrix
        val side = symbol.width + 2 * quietZoneModules
        return Array(side) { y ->
            BooleanArray(side) { x ->
                val mx = x - quietZoneModules
                val my = y - quietZoneModules
                mx in 0 until symbol.width && my in 0 until symbol.height && symbol[mx, my] == 1.toByte()
            }
        }
    }
}

/**
 * Codes as dense as bae shows, from synthetic data: coven's frame layout with
 * random bytes, base32 encoded with its prefix, and an S3 location detected
 * at setup, which carries its full settings.
 */
internal object DenseCodes {
    private const val BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    private const val HEX = "0123456789abcdef"

    // Frame prefix (kind, format version, length), store id and the trailing CRC-32C.
    private const val FRAME_BYTES = 7 + 16 + 4

    // Member id, member keys' length and keys, storage length.
    private const val RESTORE_MATERIAL_BYTES = 16 + 4 + 69 + 4

    // Store name ("Home library") and length, invitee ("Robin") with its tag
    // and length, delivery, invite id, member id, invite secret, initial key
    // id, inviting writer, read-through slot, storage length.
    private const val INVITE_MATERIAL_BYTES = 4 + 12 + 1 + 4 + 5 + 1 + 16 + 16 + 32 + 16 + 8 + 8 + 4

    private const val LOCATION =
        """{"S3":{"bucket":"music-library","region":"us-east-1","endpoint":"http://192.168.1.20:9000/",""" +
            """"profile":{"Custom":{"signing":"AwsSigV4","single_put":"IfNoneMatch",""" +
            """"copy_publication":{"Multipart":"CloudflareIfNoneMatch"},"multipart_timestamp":"Start",""" +
            """"checksums":"Absent","listing":"V1","revision":"ETag","response_headers":"S3",""" +
            """"request_checksums":false}},"prefix":""}}"""

    /** A restore code beside this device's S3 key, as the Add Device QR carries it. */
    fun restoreCodeWithKey(random: Random): String =
        "CVR1-" + base32(random, FRAME_BYTES + RESTORE_MATERIAL_BYTES + LOCATION.length) + "." +
            Base64.getUrlEncoder().withoutPadding().encodeToString(key(random).toByteArray())

    /** An in-person invite carrying the invitee's S3 key. */
    fun inviteWithKey(random: Random): String {
        val storage = """{"S3":{"invitation":{"location":$LOCATION,"acceptance":"Granted"},"credentials":${key(random)}}}"""
        return "CVI1-" + base32(random, FRAME_BYTES + INVITE_MATERIAL_BYTES + storage.length)
    }

    private fun key(random: Random) = """{"access_key_id":"${hex(random, 32)}","secret_access_key":"${hex(random, 64)}"}"""

    private fun base32(
        random: Random,
        bytes: Int,
    ) = String(CharArray((bytes * 8 + 4) / 5) { BASE32[random.nextInt(BASE32.length)] })

    private fun hex(
        random: Random,
        length: Int,
    ) = String(CharArray(length) { HEX[random.nextInt(HEX.length)] })
}

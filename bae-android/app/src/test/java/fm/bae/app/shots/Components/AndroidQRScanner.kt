package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import fm.bae.app.ui.components.AndroidQRScannerOverlay
import org.junit.Test

/**
 * The component shots of `AndroidQRScanner`, one for each row of its preview. The camera draws nothing
 * in a shot, so each row shows the scanner's own overlay over black, as the preview draws the picture.
 */
class AndroidQRScannerShots : BoardShot() {
    @Test
    fun androidComponentQrScannerInstructions() =
        component("android.component.qr-scanner.instructions") {
            overOnBlack("Scan a restore code shown in bae on one of your devices, or an invite.")
        }

    @Test
    fun androidComponentQrScannerNoInstructions() =
        component("android.component.qr-scanner.no-instructions") {
            overOnBlack(null)
        }
}

@Composable
private fun overOnBlack(instructions: String?) {
    Box(Modifier.fillMaxSize().background(Color.Black)) {
        AndroidQRScannerOverlay(
            modifier = Modifier.align(Alignment.BottomCenter),
            instructions = instructions,
            onDismiss = {},
        )
    }
}

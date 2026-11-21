package fm.bae.app.shots

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import fm.bae.app.ui.components.AndroidQRScanner
import org.junit.Test

/**
 * The component shots of `AndroidQRScanner`, one for each row of its preview. A shot has no camera, so
 * each row passes a black picture, as the preview draws it.
 */
class AndroidQRScannerShots : BoardShot() {
    @Test
    fun androidComponentQrScannerInstructions() =
        component("android.component.qr-scanner.instructions") {
            scanner("Scan a restore code shown in bae on one of your devices, or an invite.")
        }

    @Test
    fun androidComponentQrScannerNoInstructions() =
        component("android.component.qr-scanner.no-instructions") {
            scanner(null)
        }
}

@Composable
private fun scanner(instructions: String?) {
    AndroidQRScanner(
        onScanned = {},
        onDismiss = {},
        instructions = instructions,
        camera = { Box(Modifier.fillMaxSize().background(Color.Black)) },
    )
}

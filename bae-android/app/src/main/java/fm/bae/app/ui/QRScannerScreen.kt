package fm.bae.app.ui

import android.content.Context
import android.util.Size
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.google.common.util.concurrent.ListenableFuture
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import fm.bae.app.R
import java.util.concurrent.ExecutorService

private const val ANALYSIS_WIDTH = 1280
private const val ANALYSIS_HEIGHT = 720

@Composable
fun QRScannerScreen(
    onScanned: (String) -> Unit,
    onDismiss: () -> Unit,
    instructions: String = stringResource(R.string.qr_scanner_instructions),
) {
    val context = LocalContext.current
    val cameraProviderFuture = remember { ProcessCameraProvider.getInstance(context) }
    var scanned = remember { false }
    // ZXing decodes synchronously, so the analyzer runs on its own thread rather
    // than the main one; the latch and reader are only ever touched from there.
    val analysisExecutor =
        remember {
            java.util.concurrent.Executors
                .newSingleThreadExecutor()
        }
    val reader =
        remember {
            MultiFormatReader().apply {
                setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
            }
        }

    Box(modifier = Modifier.fillMaxSize()) {
        AndroidView(
            factory = { ctx ->
                createQRPreviewView(ctx, cameraProviderFuture, analysisExecutor, reader) {
                    if (!scanned) {
                        scanned = true
                        onScanned(it)
                    }
                }
            },
            modifier = Modifier.fillMaxSize(),
        )
        QRScannerOverlay(
            modifier = Modifier.align(Alignment.BottomCenter),
            instructions = instructions,
            onDismiss = onDismiss,
        )
    }

    DisposableEffect(Unit) {
        onDispose {
            try {
                cameraProviderFuture.get().unbindAll()
            } catch (_: Exception) {
            }
            analysisExecutor.shutdown()
        }
    }
}

private fun createQRPreviewView(
    context: Context,
    cameraProviderFuture: ListenableFuture<ProcessCameraProvider>,
    analysisExecutor: ExecutorService,
    reader: MultiFormatReader,
    onScanned: (String) -> Unit,
): PreviewView {
    val previewView = PreviewView(context)
    val mainExecutor = ContextCompat.getMainExecutor(context)

    cameraProviderFuture.addListener({
        val cameraProvider = cameraProviderFuture.get()
        val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
        val analyzer =
            ImageAnalysis
                .Builder()
                .setTargetResolution(Size(ANALYSIS_WIDTH, ANALYSIS_HEIGHT))
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
                .also { analysis ->
                    analysis.setAnalyzer(analysisExecutor) { imageProxy ->
                        val plane = imageProxy.planes[0]
                        val data = ByteArray(plane.buffer.remaining())
                        plane.buffer.get(data)
                        val source =
                            PlanarYUVLuminanceSource(
                                data,
                                plane.rowStride,
                                imageProxy.height,
                                0,
                                0,
                                imageProxy.width,
                                imageProxy.height,
                                false,
                            )
                        val bitmap = BinaryBitmap(HybridBinarizer(source))
                        try {
                            val text = reader.decode(bitmap).text
                            // Dispatch back to main before calling onScanned, which
                            // touches Compose state (the scanned latch in the caller).
                            mainExecutor.execute { onScanned(text) }
                        } catch (_: NotFoundException) {
                            // No QR code in this frame; keep scanning.
                        } finally {
                            reader.reset()
                        }
                        imageProxy.close()
                    }
                }
        cameraProvider.unbindAll()
        cameraProvider.bindToLifecycle(
            context as androidx.lifecycle.LifecycleOwner,
            CameraSelector.DEFAULT_BACK_CAMERA,
            preview,
            analyzer,
        )
    }, mainExecutor)

    return previewView
}

@Composable
private fun QRScannerOverlay(
    modifier: Modifier,
    instructions: String,
    onDismiss: () -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier.padding(32.dp),
    ) {
        Text(
            text = instructions,
            style = MaterialTheme.typography.bodySmall,
            color = Color.White,
            modifier = Modifier.background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(8.dp)).padding(8.dp),
        )
        Button(onClick = onDismiss, modifier = Modifier.padding(top = 16.dp)) {
            Text(stringResource(R.string.cancel))
        }
    }
}

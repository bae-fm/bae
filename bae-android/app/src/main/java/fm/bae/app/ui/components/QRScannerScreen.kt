package fm.bae.app.ui.components

import android.content.Context
import android.util.Size
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
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
import fm.bae.app.BaeLogger
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import java.util.concurrent.ExecutorService

private const val ANALYSIS_WIDTH = 1280
private const val ANALYSIS_HEIGHT = 720
private const val TAG = "QRScannerScreen"

@Composable
fun QRScannerScreen(
    onScanned: (String) -> Unit,
    onDismiss: () -> Unit,
    instructions: String? = stringResource(R.string.qr_scanner_instructions),
) {
    val context = LocalContext.current
    val cameraProviderFuture = remember { ProcessCameraProvider.getInstance(context) }
    var scanned = remember { false }
    // ZXing decodes synchronously, so frames are analyzed off the main thread;
    // only that thread touches the reader.
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
                createQRPreviewView(
                    context = ctx,
                    cameraProviderFuture = cameraProviderFuture,
                    analysisExecutor = analysisExecutor,
                    reader = reader,
                ) {
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
    val logger = BaeLogger(TAG)

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
                        analyzeQrFrame(
                            reader = reader,
                            imageProxy = imageProxy,
                            onScanned = { text ->
                                // onScanned touches Compose state, so run it on main.
                                mainExecutor.execute { onScanned(text) }
                            },
                            onDecodeFailure = { e ->
                                logger.warning("QR analyzer failed to decode frame", e)
                            },
                        )
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

internal fun analyzeQrFrame(
    reader: MultiFormatReader,
    imageProxy: ImageProxy,
    onScanned: (String) -> Unit,
    onDecodeFailure: (Exception) -> Unit,
) {
    try {
        onScanned(decodeQrFrame(reader, imageProxy))
    } catch (_: NotFoundException) {
        // No QR code in this frame; keep scanning.
    } catch (e: Exception) {
        onDecodeFailure(e)
    } finally {
        try {
            reader.reset()
        } finally {
            imageProxy.close()
        }
    }
}

private fun decodeQrFrame(
    reader: MultiFormatReader,
    imageProxy: ImageProxy,
): String {
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
    return reader.decode(bitmap).text
}

@Composable
private fun QRScannerOverlay(
    modifier: Modifier,
    instructions: String?,
    onDismiss: () -> Unit,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier.padding(ThemeSpace.page),
    ) {
        if (instructions != null) {
            Text(
                text = instructions,
                style = ThemeText.detail.style,
                color = BaeTheme.colors.onFill,
                modifier =
                    Modifier
                        .background(BaeTheme.colors.scrim, RoundedCornerShape(ThemeRadius.control))
                        .padding(ThemeSpace.related),
            )
        }
        PrimaryButton(onClick = onDismiss, modifier = Modifier.padding(top = ThemeSpace.edge)) {
            Text(stringResource(R.string.cancel))
        }
    }
}

// Previews the overlay only; the camera surface needs a live camera.
@androidx.compose.ui.tooling.preview.Preview(showBackground = true)
@Composable
private fun QRScannerOverlayPreview() {
    BaeTheme {
        QRScannerOverlay(
            modifier = Modifier,
            instructions = "Point the camera at the code",
            onDismiss = {},
        )
    }
}

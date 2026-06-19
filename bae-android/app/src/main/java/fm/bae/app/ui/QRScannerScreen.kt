package fm.bae.app.ui

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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import fm.bae.app.R
import java.util.concurrent.Executors

@Composable
fun QRScannerScreen(
    onScanned: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val cameraProviderFuture = remember { ProcessCameraProvider.getInstance(context) }
    var scanned = remember { false }
    // ZXing decodes synchronously, so the analyzer runs on its own thread rather
    // than the main one; the latch and reader are only ever touched from there.
    val analysisExecutor = remember { Executors.newSingleThreadExecutor() }
    val reader =
        remember {
            MultiFormatReader().apply {
                setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
            }
        }

    Box(modifier = Modifier.fillMaxSize()) {
        AndroidView(
            factory = { ctx ->
                val previewView = PreviewView(ctx)
                val mainExecutor = ContextCompat.getMainExecutor(ctx)

                cameraProviderFuture.addListener({
                    val cameraProvider = cameraProviderFuture.get()
                    val preview =
                        Preview.Builder().build().also {
                            it.surfaceProvider = previewView.surfaceProvider
                        }

                    val analyzer =
                        ImageAnalysis
                            .Builder()
                            .setTargetResolution(Size(1280, 720))
                            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                            .build()
                            .also { analysis ->
                                analysis.setAnalyzer(analysisExecutor) { imageProxy ->
                                    if (!scanned) {
                                        // The Y (luminance) plane of the YUV frame is all
                                        // ZXing needs; rowStride covers any row padding.
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
                                            val value = reader.decode(bitmap).text
                                            if (!scanned) {
                                                scanned = true
                                                mainExecutor.execute { onScanned(value) }
                                            }
                                        } catch (_: NotFoundException) {
                                            // No QR code in this frame; keep scanning.
                                        } finally {
                                            reader.reset()
                                        }
                                    }
                                    imageProxy.close()
                                }
                            }

                    cameraProvider.unbindAll()
                    cameraProvider.bindToLifecycle(
                        ctx as androidx.lifecycle.LifecycleOwner,
                        CameraSelector.DEFAULT_BACK_CAMERA,
                        preview,
                        analyzer,
                    )
                }, mainExecutor)

                previewView
            },
            modifier = Modifier.fillMaxSize(),
        )

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier =
                Modifier
                    .align(Alignment.BottomCenter)
                    .padding(32.dp),
        ) {
            Text(
                text = stringResource(R.string.qr_scanner_instructions),
                style = MaterialTheme.typography.bodySmall,
                color = androidx.compose.ui.graphics.Color.White,
                modifier =
                    Modifier
                        .background(
                            androidx.compose.ui.graphics.Color.Black
                                .copy(alpha = 0.6f),
                            RoundedCornerShape(8.dp),
                        ).padding(8.dp),
            )
            Button(
                onClick = onDismiss,
                modifier = Modifier.padding(top = 16.dp),
            ) {
                Text(stringResource(R.string.cancel))
            }
        }
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

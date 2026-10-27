package fm.bae.app.ui.components

import android.content.Context
import android.graphics.ImageFormat
import android.util.Size
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.AspectRatioStrategy
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
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
import fm.bae.app.BaeLogger
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeRadius
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import java.util.concurrent.ExecutorService

private const val TAG = "QRScannerScreen"
private const val NANOS_PER_MILLI = 1_000_000L
private const val SUMMARY_INTERVAL_NANOS = 1_000 * NANOS_PER_MILLI

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
    // only that thread touches the decoder.
    val analysisExecutor =
        remember {
            java.util.concurrent.Executors
                .newSingleThreadExecutor()
        }
    val decoder = remember { QrFrameDecoder() }

    Box(modifier = Modifier.fillMaxSize()) {
        AndroidView(
            factory = { ctx ->
                createQRPreviewView(
                    context = ctx,
                    cameraProviderFuture = cameraProviderFuture,
                    analysisExecutor = analysisExecutor,
                    decoder = decoder,
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
    decoder: QrFrameDecoder,
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
                // The bound size is a sensor output size, landscape whatever
                // the phone's rotation.
                .setResolutionSelector(
                    ResolutionSelector
                        .Builder()
                        .setAspectRatioStrategy(AspectRatioStrategy.RATIO_16_9_FALLBACK_AUTO_STRATEGY)
                        .setResolutionStrategy(
                            ResolutionStrategy(
                                Size(QR_ANALYSIS_WIDTH, QR_ANALYSIS_HEIGHT),
                                ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER,
                            ),
                        ).build(),
                ).setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
                .also { analysis ->
                    val tally = QrFrameTally(logger)
                    analysis.setAnalyzer(analysisExecutor) { imageProxy ->
                        tally.start(imageProxy)
                        val outcome = decoder.analyze(imageProxy)
                        tally.record(outcome)
                        when (outcome) {
                            // onScanned touches Compose state, so run it on main.
                            is QrFrameOutcome.Found -> {
                                mainExecutor.execute { onScanned(outcome.text) }
                            }

                            QrFrameOutcome.NotFound, QrFrameOutcome.Unreadable -> {}

                            is QrFrameOutcome.Failed -> {
                                logger.warning("QR analyzer failed to decode frame", outcome.error)
                            }
                        }
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

/**
 * Logs the frames the camera delivers: their shape once, when the first
 * arrives, and what they came to once a second after that, so a scan that
 * never reads a code shows what the decoder was given. Runs on the analyzer's
 * thread only.
 */
private class QrFrameTally(
    private val logger: BaeLogger,
) {
    private var started = false
    private var windowStart = 0L
    private var found = 0
    private var notFound = 0
    private var unreadable = 0
    private var failed = 0

    fun start(imageProxy: ImageProxy) {
        if (started) return
        started = true
        windowStart = System.nanoTime()
        val luma = imageProxy.planes[0]
        logger.info(
            "QR analysis frames: ${imageProxy.width}x${imageProxy.height}, " +
                "rotation ${imageProxy.imageInfo.rotationDegrees}, " +
                "format ${imageProxy.format}" +
                (if (imageProxy.format == ImageFormat.YUV_420_888) " (YUV_420_888)" else "") +
                ", luma row stride ${luma.rowStride}, pixel stride ${luma.pixelStride}",
        )
    }

    fun record(outcome: QrFrameOutcome) {
        when (outcome) {
            is QrFrameOutcome.Found -> found++
            QrFrameOutcome.NotFound -> notFound++
            QrFrameOutcome.Unreadable -> unreadable++
            is QrFrameOutcome.Failed -> failed++
        }
        val elapsed = System.nanoTime() - windowStart
        if (outcome is QrFrameOutcome.Found || elapsed >= SUMMARY_INTERVAL_NANOS) {
            logger.info(
                "QR frames in ${elapsed / NANOS_PER_MILLI} ms: ${found + notFound + unreadable + failed} analysed, " +
                    "$found read, $notFound without a code, $unreadable unreadable, $failed failed",
            )
            windowStart += elapsed
            found = 0
            notFound = 0
            unreadable = 0
            failed = 0
        }
    }
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

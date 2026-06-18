package fm.bae.app.ui

import android.util.Log
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeGalleryItem

private const val TAG = "bae.GalleryDialog"

/**
 * Full-screen artwork viewer over a release's gallery items (cover first, then
 * every image file the release has). Swipeable when there's more than one. An
 * item already on disk renders from its [BridgeGalleryItem.localPath]; a
 * cloud-only item (no local path) is fetched on demand via [loadImage], keyed by
 * the item's id.
 */
@Composable
fun GalleryDialog(
    items: List<BridgeGalleryItem>,
    loadImage: suspend (fileId: String) -> ByteArray,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        val pagerState = rememberPagerState(pageCount = { items.size })
        Surface(modifier = Modifier.fillMaxSize(), color = Color.Black) {
            Box(modifier = Modifier.fillMaxSize()) {
                HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
                    val item = items[page]
                    val localPath = item.localPath
                    if (localPath != null) {
                        GalleryImage(model = coverModel(localPath), contentDescription = item.label)
                    } else {
                        RemoteGalleryImage(
                            fileId = item.id,
                            label = item.label,
                            loadImage = loadImage,
                        )
                    }
                }
                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier.align(Alignment.TopEnd).padding(8.dp),
                ) {
                    Icon(
                        imageVector = Icons.Filled.Close,
                        contentDescription = "Close",
                        tint = Color.White,
                    )
                }
                // Current item's label (e.g. "Cover", "Back.jpg") + page counter,
                // so multi-image galleries aren't a blind swipe-through.
                Column(
                    modifier = Modifier.align(Alignment.BottomCenter).padding(16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    // Core always sets a non-empty label ("Cover" or the file's
                    // original filename), so render it unconditionally.
                    Text(text = items[pagerState.currentPage].label, color = Color.White)
                    if (items.size > 1) {
                        Text(
                            text = "${pagerState.currentPage + 1} / ${items.size}",
                            color = Color.White.copy(alpha = 0.7f),
                        )
                    }
                }
            }
        }
    }
}

/** A gallery image filling the page: cover-from-path or fetched cloud bytes. */
@Composable
private fun GalleryImage(model: Any?, contentDescription: String?) {
    AsyncImage(
        model = model,
        contentDescription = contentDescription,
        contentScale = ContentScale.Fit,
        modifier = Modifier.fillMaxSize(),
    )
}

/**
 * A gallery image whose file isn't on disk here: fetch its bytes (downloaded
 * from the release's cloud home and decrypted by core) and render them, showing
 * a spinner while loading and the failure reason if the fetch fails. `result`
 * is null while loading, then success(bytes) or failure(error).
 */
@Composable
private fun RemoteGalleryImage(
    fileId: String,
    label: String,
    loadImage: suspend (fileId: String) -> ByteArray,
) {
    var result: Result<ByteArray>? by remember(fileId) {
        mutableStateOf<Result<ByteArray>?>(null)
    }
    LaunchedEffect(fileId) {
        result = try {
            Result.success(withContext(Dispatchers.IO) { loadImage(fileId) })
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Failed to load gallery image $fileId", e)
            Result.failure(e)
        }
    }
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val r = result
        when {
            r == null -> CircularProgressIndicator(color = Color.White)
            r.isSuccess -> GalleryImage(model = r.getOrThrow(), contentDescription = label)
            // The underlying failure is already logged above; the viewer shows a
            // fixed, user-facing message rather than a raw exception string.
            else ->
                Text(
                    text = "Couldn't load image",
                    color = Color.White,
                    modifier = Modifier.padding(24.dp),
                )
        }
    }
}

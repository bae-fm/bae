package fm.bae.app.ui.components

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.data.DecodeSize
import fm.bae.app.data.ImageContent
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CancellationException
import uniffi.bae_bridge.BridgeImageRef

private const val TAG = "bae.CoverImage"
private val logger = BaeLogger(TAG)

private sealed interface SlotState {
    data object Loading : SlotState

    data object Absent : SlotState

    data class Loaded(
        val bitmap: Bitmap,
    ) : SlotState
}

/**
 * Album-cover thumbnail: the image [cover] names, clipped to a rounded square, or
 * a MusicNote placeholder when there is no cover. The caller's [modifier] carries
 * the sizing — `Modifier.size(48.dp)` for list rows,
 * `Modifier.fillMaxWidth().aspectRatio(1f)` for full-width art.
 */
@Composable
fun CoverImage(
    cover: BridgeImageRef?,
    cornerRadius: Dp,
    iconPadding: Dp,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
) {
    ImageSlot(
        content = cover?.let { ImageContent.LibraryImage(it) },
        cornerRadius = cornerRadius,
        iconPadding = iconPadding,
        modifier = modifier,
        contentDescription = contentDescription,
    )
}

/**
 * One image slot: renders whatever [ImageStore] resolves [content] to, at the
 * pixel size the slot is laid out at. Pure renderer — it fetches nothing, caches
 * nothing, and decodes nothing itself.
 *
 * The first frame after a (re)mount draws from the store's decoded cache
 * synchronously, so a row scrolled back into view shows its art immediately
 * instead of flashing a placeholder while an async load lands a frame later.
 */
@Composable
fun ImageSlot(
    content: ImageContent?,
    cornerRadius: Dp,
    iconPadding: Dp,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
) {
    val store = LocalImageStore.current
    BoxWithConstraints(
        modifier = modifier.clip(RoundedCornerShape(cornerRadius)),
        contentAlignment = Alignment.Center,
    ) {
        val size = DecodeSize.FitTo(boundedPixelSize(constraints))
        var state by remember(content, size) {
            mutableStateOf(
                when {
                    content == null -> SlotState.Absent
                    else ->
                        store.cachedImage(content, size)?.let { SlotState.Loaded(it) }
                            ?: SlotState.Loading
                },
            )
        }
        LaunchedEffect(content, size) {
            if (content == null || state is SlotState.Loaded) return@LaunchedEffect
            if (size.pixels <= 0) {
                // An image slot with no bounded dimension can't say how large to
                // decode; it reads the source whole. Layout, not the store, is
                // what would fix it.
                logger.warning("image slot for ${content.description} has unbounded constraints")
            }
            state =
                try {
                    store.image(content, size)?.let { SlotState.Loaded(it) }
                        ?: SlotState.Absent
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logger.error("Failed to load ${content.description}", e)
                    SlotState.Absent
                }
        }
        when (val current = state) {
            is SlotState.Loaded -> {
                Image(
                    bitmap = current.bitmap.asImageBitmap(),
                    contentDescription = contentDescription,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }

            // An image exists but its bytes aren't in yet: a plain tile (no glyph),
            // so the art pops in without a placeholder flash beforehand.
            SlotState.Loading -> {
                CoverTile(showIcon = false, iconPadding = iconPadding)
            }

            // No image, or its bytes were absent/failed (logged above).
            SlotState.Absent -> {
                CoverTile(showIcon = true, iconPadding = iconPadding)
            }
        }
    }
}

/**
 * The pixel size a slot with these [constraints] should decode to: its longer
 * bounded edge, or 0 when neither edge is bounded. Both dimensions count, so a
 * wide-but-short slot still decodes enough pixels to fill it.
 */
private fun boundedPixelSize(constraints: Constraints): Int {
    val width = if (constraints.hasBoundedWidth) constraints.maxWidth else 0
    val height = if (constraints.hasBoundedHeight) constraints.maxHeight else 0
    return maxOf(width, height)
}

/** The placeholder tile behind/instead of a cover: a flat surface, with the
 *  MusicNote glyph only when there is no cover to show. */
@Composable
private fun CoverTile(
    showIcon: Boolean,
    iconPadding: Dp,
) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceVariant,
        modifier = Modifier.fillMaxSize(),
    ) {
        if (showIcon) {
            Icon(
                imageVector = Icons.Filled.MusicNote,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(iconPadding),
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun CoverImagePreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore()) {
            CoverImage(
                cover = null,
                cornerRadius = 6.dp,
                iconPadding = 24.dp,
                modifier = Modifier.size(120.dp),
            )
        }
    }
}

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
 * Album cover clipped to a rounded square, or a music-note placeholder when there
 * is none; [modifier] sets the size.
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
 * Draws what [ImageStore] resolves [content] to at the slot's laid-out pixel size,
 * starting from the store's cache so a remounted row shows its art immediately.
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
        var state by remember(content, size) { mutableStateOf(store.firstFrameState(content, size)) }
        LaunchedEffect(content, size) {
            if (content != null && state !is SlotState.Loaded) {
                state = store.loadedState(content, size)
            }
        }
        SlotContent(
            state = state,
            iconPadding = iconPadding,
            contentDescription = contentDescription,
        )
    }
}

/** What the slot draws before any load runs: the decode the store already holds. */
private fun ImageStore.firstFrameState(
    content: ImageContent?,
    size: DecodeSize.FitTo,
): SlotState =
    when (content) {
        null -> SlotState.Absent
        else -> cachedImage(content, size)?.let { SlotState.Loaded(it) } ?: SlotState.Loading
    }

/** [content] decoded at [size], or Absent when there are no bytes or the load
 *  failed (logged here). */
private suspend fun ImageStore.loadedState(
    content: ImageContent,
    size: DecodeSize.FitTo,
): SlotState {
    if (size.pixels <= 0) {
        // Unbounded slots decode the source whole; the layout needs fixing.
        logger.warning("image slot for ${content.description} has unbounded constraints")
    }
    return try {
        image(content, size)?.let { SlotState.Loaded(it) } ?: SlotState.Absent
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        logger.error("Failed to load ${content.description}", e)
        SlotState.Absent
    }
}

/** The decoded art, or the placeholder tile standing in for it. */
@Composable
private fun SlotContent(
    state: SlotState,
    iconPadding: Dp,
    contentDescription: String?,
) {
    when (state) {
        is SlotState.Loaded -> {
            Image(
                bitmap = state.bitmap.asImageBitmap(),
                contentDescription = contentDescription,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        }

        // Bytes not in yet: a plain tile, so no glyph flashes before the art.
        SlotState.Loading -> {
            CoverTile(showIcon = false, iconPadding = iconPadding)
        }

        // No image, or its bytes were absent or failed to load.
        SlotState.Absent -> {
            CoverTile(showIcon = true, iconPadding = iconPadding)
        }
    }
}

/** The longer bounded edge of [constraints] in pixels, or 0 when neither is bounded. */
private fun boundedPixelSize(constraints: Constraints): Int {
    val width = if (constraints.hasBoundedWidth) constraints.maxWidth else 0
    val height = if (constraints.hasBoundedHeight) constraints.maxHeight else 0
    return maxOf(width, height)
}

/** The placeholder surface, with the music-note glyph only when there is no cover. */
@Composable
private fun CoverTile(
    showIcon: Boolean,
    iconPadding: Dp,
) {
    Surface(
        color = BaeTheme.surfaces.placeholder,
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

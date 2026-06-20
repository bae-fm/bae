package fm.bae.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.Dp
import coil3.compose.AsyncImage
import coil3.compose.LocalPlatformContext
import coil3.request.ImageRequest
import java.io.File

/**
 * The on-disk file for a bridge cover identifier. The bridge appends a
 * `#v=<mtime>` cache-busting suffix (see `image_path_if_exists`); the actual
 * file is the part before it, so strip the suffix before any filesystem access.
 */
fun coverFile(identifier: String): File = File(identifier.substringBefore("#v="))

/**
 * A Coil model for a bridge cover identifier: it loads from [coverFile] but keys
 * the cache on the full identifier, so replacing a cover in place (same path,
 * new mtime → new `#v=` suffix) invalidates the cached image and reloads.
 */
@Composable
fun coverModel(identifier: String): ImageRequest {
    val context = LocalPlatformContext.current
    return remember(identifier) {
        ImageRequest
            .Builder(context)
            .data(coverFile(identifier))
            .memoryCacheKey(identifier)
            .diskCacheKey(identifier)
            .build()
    }
}

/**
 * Album-cover thumbnail: the local image at [path] clipped to a rounded square,
 * or a MusicNote placeholder when [path] is null. The caller's [modifier]
 * carries the sizing — `Modifier.size(48.dp)` for list rows,
 * `Modifier.fillMaxWidth().aspectRatio(1f)` for full-width art.
 */
@Composable
fun CoverImage(
    path: String?,
    cornerRadius: Dp,
    iconPadding: Dp,
    modifier: Modifier = Modifier,
    contentDescription: String? = null,
) {
    Box(
        modifier = modifier.clip(RoundedCornerShape(cornerRadius)),
        contentAlignment = Alignment.Center,
    ) {
        if (path != null) {
            AsyncImage(
                model = coverModel(path),
                contentDescription = contentDescription,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Surface(
                color = MaterialTheme.colorScheme.surfaceVariant,
                modifier = Modifier.fillMaxSize(),
            ) {
                Icon(
                    imageVector = Icons.Filled.MusicNote,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(iconPadding),
                )
            }
        }
    }
}

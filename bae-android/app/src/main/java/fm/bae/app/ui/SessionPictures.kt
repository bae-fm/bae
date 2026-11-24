package fm.bae.app.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.OpenLibrary
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.LocalPictureTileStore

/** [content] with the session's image and tile stores, scoped to it because their reads and the image cache are
 *  keyed on that library's ids. */
@Composable
internal fun SessionPictures(
    session: OpenLibrary,
    content: @Composable () -> Unit,
) = CompositionLocalProvider(
    LocalImageStore provides session.imageStore,
    LocalPictureTileStore provides session.pictureTiles,
    content = content,
)

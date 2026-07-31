package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData

/**
 * The `album-detail` screenshot scene: the album detail body for a fixture
 * release, wrapped in the app chrome — the shared composition the capture and
 * the dev preview below both render. Uses the production [AlbumDetailContent]
 * with inert callbacks and no download control (that control needs a live
 * session, and core offers none for a local fixture release). The store resolves
 * nothing, so covers show the placeholder icon.
 *
 * The side header and track duration are pre-computed fields on the fixture rows
 * (a null header key for the flat side, a clock built in-process), so they
 * render under the preview renderer with no bridge/FFI call.
 */
@Composable
internal fun AlbumDetailScene() {
    BaeAppChrome {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            AlbumDetailContent(
                detail = PreviewData.albumDetail(),
                selectedRelease = PreviewData.release(),
                cover = PreviewData.imageRef(),
                playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
                callbacks = inertAlbumDetailCallbacks(),
                releaseDownloadControl = {},
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun AlbumDetailScenePreview() {
    AlbumDetailScene()
}

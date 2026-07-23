package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.PreviewData

/**
 * The `album-detail` screenshot scene: the album detail body for a fixture
 * release, wrapped in the app chrome — the shared composition the capture and
 * the dev preview below both render. Uses the production [AlbumDetailContent]
 * with inert callbacks and no download control (that control needs a live
 * session, and core offers none for a local fixture release). No image bytes
 * load, so covers show the placeholder icon.
 */
@Composable
internal fun AlbumDetailScene() {
    BaeAppChrome {
        AlbumDetailContent(
            detail = PreviewData.albumDetail(),
            selectedRelease = PreviewData.release(),
            cover = PreviewData.imageRef(),
            playback = AlbumPlaybackState(currentTrackId = null, isPlaying = false),
            callbacks = inertAlbumDetailCallbacks(),
            releaseDownloadControl = {},
            // The side header and track duration reach bae-core over the bridge
            // (JNA/FFI), which can't run under the preview renderer; stub them
            // like the cover loader above. The fixture is a single flat side, so
            // the real side header is empty here too — the stub matches what ships.
            trackLabels = AlbumTrackLabels(sideHeader = { "" }, trackDuration = { "" }),
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AlbumDetailScenePreview() {
    AlbumDetailScene()
}

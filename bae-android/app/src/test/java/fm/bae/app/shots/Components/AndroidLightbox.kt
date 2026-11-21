package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.TestImages
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.GalleryViewer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import org.junit.Test
import uniffi.bae_bridge.BridgeGalleryLabel

/** The component shots of `AndroidLightbox`, one for each row of its preview. */
class AndroidLightboxShots : BoardShot() {
    @Test
    fun androidComponentLightboxImage() = shot("android.component.lightbox.image") { Lightbox(shownImage(), count = 4u) }

    @Test
    fun androidComponentLightboxLoading() = shot("android.component.lightbox.loading") { Lightbox(neverLoaded(), count = 4u) }

    @Test
    fun androidComponentLightboxOneImage() = shot("android.component.lightbox.one-image") { Lightbox(shownImage(), count = 1u) }
}

/** The lightbox on one gallery item, the release's cover.jpg, of [count] images, reading its bytes from [store]. */
@Composable
private fun Lightbox(
    store: ImageStore,
    count: UInt,
) = ScreenScene {
    CompositionLocalProvider(LocalImageStore provides store, LocalBugReports provides BugReports()) {
        GalleryViewer(
            releaseId = "rel-1",
            items = listOf(PreviewData.galleryItem(label = BridgeGalleryLabel.File("cover.jpg"))),
            count = count,
            onDismiss = {},
        )
    }
}

/** A store whose release image is a picture. */
private fun shownImage() =
    ImageStore(
        fetchReleaseImageBytes = {
            _,
            _,
            ->
            TestImages.quadrantsJpeg(400, listOf(0xFF2D4A6B.toInt(), 0xFF6B8EAD.toInt(), 0xFFC9A66B.toInt(), 0xFF8C3B4A.toInt()))
        },
        dispatcher = Dispatchers.Unconfined,
    )

/** A store whose bytes never arrive, so the lightbox stays on its load. */
private fun neverLoaded() = ImageStore(fetchReleaseImageBytes = { _, _ -> awaitCancellation() }, dispatcher = Dispatchers.Unconfined)

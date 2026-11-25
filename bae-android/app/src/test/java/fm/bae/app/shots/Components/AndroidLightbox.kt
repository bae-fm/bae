package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.LocalPictureTileStore
import fm.bae.app.data.PictureTileStore
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.GalleryFailure
import fm.bae.app.ui.albumdetail.GalleryFailures
import fm.bae.app.ui.albumdetail.GalleryViewer
import fm.bae.app.ui.albumdetail.LocalGalleryFailures
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
    fun androidComponentLightboxFailed() = shot("android.component.lightbox.failed") { Lightbox(noBytes(), count = 4u) }

    @Test
    fun androidComponentLightboxFailedDetails() =
        shot("android.component.lightbox.failed-details") { Lightbox(failing(), count = 4u, failures = withCauses) }

    @Test
    fun androidComponentLightboxOneImage() = shot("android.component.lightbox.one-image") { Lightbox(shownImage(), count = 1u) }
}

/** The lightbox on one gallery item, the release's cover.jpg, of [count] images, reading its bytes from [store]. */
@Composable
private fun Lightbox(
    store: ImageStore,
    count: UInt,
    failures: GalleryFailures = LocalGalleryFailures.current,
) = ScreenScene {
    CompositionLocalProvider(
        LocalImageStore provides store,
        LocalGalleryFailures provides failures,
        LocalPictureTileStore provides PictureTileStore(FakeAppHandle()),
        LocalBugReports provides BugReports(),
    ) {
        GalleryViewer(
            releaseId = "rel-1",
            title = "Low Tide Hours",
            items = listOf(PreviewData.galleryItem(label = BridgeGalleryLabel.File("cover.jpg"))),
            count = count,
            onDismiss = {},
        )
    }
}

/** A store whose release image is the board's cover for Low Tide Hours, the artwork the other shots draw. */
private fun shownImage() =
    ImageStore(
        fetchReleaseImageBytes = {
            _,
            _,
            _,
            ->
            requireNotNull(BoardShot::class.java.getResourceAsStream("/Low Tide Hours.png")) { "No board artwork for Low Tide Hours" }
                .use { it.readBytes() }
        },
        dispatcher = Dispatchers.Unconfined,
    )

/** A store whose bytes never arrive, so the lightbox stays on its load. */
private fun neverLoaded() = ImageStore(fetchReleaseImageBytes = { _, _, _ -> awaitCancellation() }, dispatcher = Dispatchers.Unconfined)

/** A store with no bytes for the image, so the lightbox shows the failure with Retry and no Details. */
private fun noBytes() = ImageStore(dispatcher = Dispatchers.Unconfined)

/** A store whose read of the image fails, so the lightbox shows its failure. */
private fun failing() =
    ImageStore(
        fetchReleaseImageBytes = { _, _, _ -> throw IllegalStateException("the read failed") },
        dispatcher = Dispatchers.Unconfined,
    )

/** What core's words and causes for that failure are, which the shot states itself: the bridge is not loaded here. */
private val withCauses =
    GalleryFailures { _, _, _, _ ->
        GalleryFailure(DisplayError("Could not reach the cloud.", "dns error"), offersRetry = true)
    }

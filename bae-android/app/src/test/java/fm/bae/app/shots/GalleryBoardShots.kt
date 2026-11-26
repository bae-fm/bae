package fm.bae.app.shots

import org.junit.Test

/** The shots of the gallery's boards: the gallery on its first of four images after the picture failed. */
class GalleryBoardShots : BaeBoardShot() {
    @Test
    fun androidAlbumGalleryZoomFailed() = shot("android.album.gallery.zoom-failed") { Lightbox(failing(), count = 4u) }
}

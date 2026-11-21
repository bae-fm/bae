package fm.bae.app.shots

import fm.bae.app.ui.albumdetail.AndroidGalleryCaption
import org.junit.Test

/** The component shots of `AndroidGalleryCaption`, one for each row of its preview. */
class AndroidGalleryCaptionShots : BoardShot() {
    @Test
    fun androidComponentGalleryCaptionSeveralImages() =
        component("android.component.gallery-caption.several-images", wide = true) {
            OnTheBackdrop { AndroidGalleryCaption(label = "cover.jpg", page = 1, count = 4u) }
        }

    @Test
    fun androidComponentGalleryCaptionOneImage() =
        component("android.component.gallery-caption.one-image", wide = true) {
            OnTheBackdrop { AndroidGalleryCaption(label = "cover.jpg", page = 1, count = 1u) }
        }
}

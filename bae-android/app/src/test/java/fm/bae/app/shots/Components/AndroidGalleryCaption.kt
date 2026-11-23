package fm.bae.app.shots

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.albumdetail.AndroidGalleryCaption
import org.junit.Test

/** The component shots of `AndroidGalleryCaption`, one for each row of its preview. */
class AndroidGalleryCaptionShots : BoardShot() {
    @Test
    fun androidComponentGalleryCaptionSeveralImages() =
        component("android.component.gallery-caption.several-images") {
            GalleryCaption(count = 4u)
        }

    @Test
    fun androidComponentGalleryCaptionOneImage() =
        component("android.component.gallery-caption.one-image") {
            GalleryCaption(count = 1u)
        }
}

@Composable
private fun GalleryCaption(count: UInt) {
    OnTheBackdrop { AndroidGalleryCaption(label = "cover.jpg", page = 1, count = count, modifier = Modifier.fillMaxWidth()) }
}

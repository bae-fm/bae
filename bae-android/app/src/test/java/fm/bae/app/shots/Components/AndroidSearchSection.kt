package fm.bae.app.shots

import fm.bae.app.ui.library.AndroidSearchSection
import org.junit.Test

/** The component shots of `AndroidSearchSection`, one for each row of its preview. */
class AndroidSearchSectionShots : BaeBoardShot() {
    @Test
    fun androidComponentSearchSectionDefault() =
        component("android.component.search-section.default") {
            InsidePreviewPage { AndroidSearchSection("Albums") }
        }
}

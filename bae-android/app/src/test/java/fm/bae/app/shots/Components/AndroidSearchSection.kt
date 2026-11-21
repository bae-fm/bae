package fm.bae.app.shots

import fm.bae.app.ui.library.AndroidSearchSection
import org.junit.Test

/** The component shots of `AndroidSearchSection`, one for each row of its preview. */
class AndroidSearchSectionShots : BoardShot() {
    @Test
    fun androidComponentSearchSectionDefault() =
        component("android.component.search-section.default", wide = true) {
            AndroidSearchSection("Albums")
        }
}

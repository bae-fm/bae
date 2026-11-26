package fm.bae.app.shots

import fm.bae.app.ui.library.AndroidSearchBar
import org.junit.Test

/** The component shots of `AndroidSearchBar`, one for each row of its preview. */
class AndroidSearchBarShots : BaeBoardShot() {
    @Test
    fun androidComponentSearchBarEmpty() =
        component("android.component.search-bar.empty") { AndroidSearchBar(query = "", onQueryChange = {}, onClose = {}) }

    @Test
    fun androidComponentSearchBarTyped() =
        component("android.component.search-bar.typed") {
            AndroidSearchBar(query = "harbor", onQueryChange = {}, onClose = {})
        }
}

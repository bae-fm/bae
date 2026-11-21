package fm.bae.app.shots

import fm.bae.app.ui.library.AndroidArtistOtherNames
import org.junit.Test

/** The component shots of `AndroidArtistOtherNames`, one for each row of its preview. */
class AndroidArtistOtherNamesShots : BoardShot() {
    @Test
    fun androidComponentArtistOtherNamesOneName() =
        component("android.component.artist-other-names.one-name", wide = true) { AndroidArtistOtherNames(listOf("Slow Orchards")) }

    @Test
    fun androidComponentArtistOtherNamesTwoNames() =
        component("android.component.artist-other-names.two-names", wide = true) {
            AndroidArtistOtherNames(listOf("Slow Orchards", "The Slow Orchards Band"))
        }
}

package fm.bae.app

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import java.util.Locale

/** A spoken list takes the locale's own joining words and punctuation, in the order given. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SpokenListTextTest {
    private fun text(
        tag: String,
        vararg items: String,
    ) = spokenListText(items.toList(), Locale.forLanguageTag(tag))

    @Test
    fun aListReadsWithTheLocalesJoiningWords() {
        assertEquals("Music 3 GB, Other 1 GB, and Free 2 GB", text("en-US", "Music 3 GB", "Other 1 GB", "Free 2 GB"))
        assertEquals("Musique 3 Go, Autre 1 Go et Libre 2 Go", text("fr-FR", "Musique 3 Go", "Autre 1 Go", "Libre 2 Go"))
    }

    @Test
    fun aSingleItemReadsAsItself() {
        assertEquals("Music 3 GB", text("en-US", "Music 3 GB"))
    }
}

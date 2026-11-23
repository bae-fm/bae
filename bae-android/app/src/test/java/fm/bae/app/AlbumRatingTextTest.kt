package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeHearter
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

/** These tests check how the album page names who rated or hearted an album and which rating a tap on a star sets. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class AlbumRatingTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Test
    fun theLineNamesThisPersonFirstThenEachOtherByNameAndStars() {
        val rating =
            BridgeAlbumRating.Stars(
                mine = BridgeStars.FOUR,
                raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
            )

        assertEquals("You · Ana rated it 5", context.ratersText(rating))
    }

    @Test
    fun anAlbumNobodyRatedHasNoLine() {
        val rating = BridgeAlbumRating.Stars(mine = null, raters = emptyList())

        assertEquals("", context.ratersText(rating))
    }

    @Test
    fun tappingTheStarsThePersonGaveClearsTheRating() {
        val rating = BridgeAlbumRating.Stars(mine = BridgeStars.FOUR, raters = emptyList())

        assertEquals(BridgeStars.TWO, rating.starsAfterTapping(BridgeStars.TWO))
        assertNull(rating.starsAfterTapping(BridgeStars.FOUR))
    }

    @Test
    fun theHeartLineNamesThisPersonFirstThenEachOtherByName() {
        val hearters = listOf(BridgeHearter.You, BridgeHearter.Person("Ana"))

        assertEquals("You · Ana hearted it", context.heartersText(hearters))
    }
}

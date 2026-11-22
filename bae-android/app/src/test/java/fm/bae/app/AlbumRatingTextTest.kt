package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

/**
 * These tests check how the album page names who rated an album, which rating a tap on a star sets or clears, and
 * which ratings the Heart system shows as a heart.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class AlbumRatingTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    @Test
    fun theLineNamesThisPersonFirstThenEachOtherByNameAndStars() {
        val rating =
            BridgeAlbumRating(
                mine = BridgeStars.FOUR,
                raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
            )

        assertEquals("You · Ana rated it 5", context.ratersText(rating))
    }

    @Test
    fun anAlbumNobodyRatedHasNoLine() {
        val rating = BridgeAlbumRating(mine = null, raters = emptyList())

        assertEquals("", context.ratersText(rating))
    }

    @Test
    fun tappingTheStarsThePersonGaveClearsTheRating() {
        val rating = BridgeAlbumRating(mine = BridgeStars.FOUR, raters = emptyList())

        assertEquals(BridgeStars.TWO, rating.starsAfterTapping(BridgeStars.TWO))
        assertNull(rating.starsAfterTapping(BridgeStars.FOUR))
    }

    @Test
    fun aRatingOfThreeStarsOrMoreIsAHeart() {
        assertEquals(false, BridgeAlbumRating(mine = null, raters = emptyList()).isHearted())
        assertEquals(false, BridgeAlbumRating(mine = BridgeStars.TWO, raters = emptyList()).isHearted())
        assertEquals(true, BridgeAlbumRating(mine = BridgeStars.THREE, raters = emptyList()).isHearted())
    }

    @Test
    fun theHeartLineNamesOnlyThoseWhoGaveThreeStarsOrMore() {
        val rating =
            BridgeAlbumRating(
                mine = BridgeStars.FIVE,
                raters =
                    listOf(
                        BridgeRater.You,
                        BridgeRater.Person("Ana", BridgeStars.THREE),
                        BridgeRater.Person("Ben", BridgeStars.TWO),
                    ),
            )

        assertEquals("You · Ana hearted it", context.heartersText(rating))
    }

    @Test
    fun theHeartLineLeavesOutThisPersonWhoseRatingIsNotAHeart() {
        val rating =
            BridgeAlbumRating(
                mine = BridgeStars.ONE,
                raters = listOf(BridgeRater.You, BridgeRater.Person("Ana", BridgeStars.FIVE)),
            )

        assertEquals("Ana hearted it", context.heartersText(rating))
    }

    @Test
    fun heartingStoresFiveStarsAndRemovingTheHeartClearsTheRating() {
        assertEquals(BridgeStars.FIVE, BridgeAlbumRating(mine = null, raters = emptyList()).starsAfterHearting())
        assertEquals(BridgeStars.FIVE, BridgeAlbumRating(mine = BridgeStars.ONE, raters = emptyList()).starsAfterHearting())
        assertNull(BridgeAlbumRating(mine = BridgeStars.THREE, raters = emptyList()).starsAfterHearting())
    }
}

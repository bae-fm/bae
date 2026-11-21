package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.albumdetail.AndroidAlbumRating
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

/** The component shots of `AndroidAlbumRating`, one for each row of its preview. */
class AndroidAlbumRatingShots : BoardShot() {
    @Test
    fun androidComponentAlbumRatingNotRated() = component("android.component.album-rating.not-rated") { Rating(mine = null) }

    @Test
    fun androidComponentAlbumRatingRated() = component("android.component.album-rating.rated") { Rating(BridgeStars.FOUR, BridgeRater.You) }

    @Test
    fun androidComponentAlbumRatingRatedByOthers() =
        component("android.component.album-rating.rated-by-others") { Rating(BridgeStars.FOUR, BridgeRater.You, anaRatedFive) }

    @Test
    fun androidComponentAlbumRatingOthersOnly() = component("android.component.album-rating.others-only") { Rating(null, anaRatedFive) }
}

/** The album's stars at [mine], over the line that names the [raters]. */
@Composable
private fun Rating(
    mine: BridgeStars?,
    vararg raters: BridgeRater,
) = AndroidAlbumRating(BridgeAlbumRating(mine = mine, raters = raters.toList()), onRate = {})

private val anaRatedFive = BridgeRater.Person("Ana", BridgeStars.FIVE)

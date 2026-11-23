package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeHearter
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

/**
 * Who rated the album: "You" for this person, then "Ana rated it 5" for each other, in the order core
 * read them, joined by the fact separator.
 */
fun Context.ratersText(rating: BridgeAlbumRating.Stars): String =
    rating.raters.joinToString(factSeparator()) { rater ->
        when (rater) {
            BridgeRater.You -> getString(R.string.members_you)
            is BridgeRater.Person -> getString(R.string.album_rated_by, rater.name, rater.stars.number)
        }
    }

/**
 * Returns the stars to set when the person taps the [tapped] star, or null to clear the rating when [tapped] is
 * the rating the person already gave.
 */
fun BridgeAlbumRating.Stars.starsAfterTapping(tapped: BridgeStars): BridgeStars? = if (tapped == mine) null else tapped

/**
 * Who has a heart on the album: "You" for this person, then "Ana hearted it" for each other, in the order core read
 * them, joined by the fact separator.
 */
fun Context.heartersText(hearters: List<BridgeHearter>): String =
    hearters.joinToString(factSeparator()) { hearter ->
        when (hearter) {
            BridgeHearter.You -> getString(R.string.members_you)
            is BridgeHearter.Person -> getString(R.string.album_hearted_by, hearter.name)
        }
    }

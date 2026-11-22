package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

// This file writes who rated an album in the person's language, as `BridgeAlbumRating+Text` does on macOS.

/**
 * Who rated the album: "You" for this person, then "Ana rated it 5" for each other, in the order core
 * read them, joined by the fact separator.
 */
fun Context.ratersText(rating: BridgeAlbumRating): String =
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
fun BridgeAlbumRating.starsAfterTapping(tapped: BridgeStars): BridgeStars? = if (tapped == mine) null else tapped

/** The stars at which an album or a track shows a heart under the Heart rating system. */
const val HEART_MINIMUM_STARS = 3

/** Whether this person's rating shows a heart: three stars or more. Fewer stars stay stored and show no heart. */
fun BridgeAlbumRating.isHearted(): Boolean = (mine?.number ?: 0) >= HEART_MINIMUM_STARS

/**
 * Who has a heart on the album: "You" for this person when they hearted it, then "Ana hearted it" for each other
 * person who rated it three stars or more, in the order core read them, joined by the fact separator.
 */
fun Context.heartersText(rating: BridgeAlbumRating): String =
    rating.raters
        .filter { rater ->
            when (rater) {
                BridgeRater.You -> rating.isHearted()
                is BridgeRater.Person -> rater.stars.number >= HEART_MINIMUM_STARS
            }
        }.joinToString(factSeparator()) { rater ->
            when (rater) {
                BridgeRater.You -> getString(R.string.members_you)
                is BridgeRater.Person -> getString(R.string.album_hearted_by, rater.name)
            }
        }

/** The stars to set when the person uses the album's heart: five, or null to clear a rating that is a heart. */
fun BridgeAlbumRating.starsAfterHearting(): BridgeStars? = if (isHearted()) null else BridgeStars.FIVE

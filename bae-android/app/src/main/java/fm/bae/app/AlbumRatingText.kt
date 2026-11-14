package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeStars

// Who rated an album, in the person's language. Mirrors macOS's `BridgeAlbumRating+Text`.

/**
 * Who rated the album: "You" for this person, then "Ana rated it 5" for each other, in the order core
 * read them, joined by the fact separator.
 */
fun Context.ratersText(rating: BridgeAlbumRating): String =
    rating.raters.joinToString(coreString("core.audio.list_separator")) { rater ->
        when (rater) {
            BridgeRater.You -> getString(R.string.members_you)
            is BridgeRater.Person -> getString(R.string.album_rated_by, rater.name, rater.stars.number)
        }
    }

/** The stars a tap on the [tapped] star asks for: the stars the person already gave clear the rating. */
fun BridgeAlbumRating.starsAfterTapping(tapped: Int): BridgeStars? {
    val stars = starsOfNumber(tapped)
    return if (stars == mine) null else stars
}

package fm.bae.app.playback

/**
 * The identity of a node in the media-browse tree the [PlaybackService] serves
 * to Android Auto / Bluetooth head units. Each node has a stable string
 * [mediaId] that the browse client round-trips: it asks for a node's children
 * by its parent's [mediaId], and asks to play a track by the track node's
 * [mediaId]. [parse] turns a client-supplied id back into the typed node.
 *
 * The encoding is `<prefix>:<payload>`. A payload is a library id whose bytes
 * we don't constrain, so a [Track], which names two of them, spells the length
 * of its release id first: splitting on the first `:` after the prefix recovers
 * that length, and the release id and track id follow it (colons and all).
 */
internal sealed interface BrowseId {
    val mediaId: String

    /** The tree root. Its children are the top-level categories. */
    data object Root : BrowseId {
        override val mediaId: String get() = ROOT
    }

    /** The albums category. Its children page through the album list. */
    data object Albums : BrowseId {
        override val mediaId: String get() = ALBUMS
    }

    /** The composers category. Its children page through the composer list. */
    data object Composers : BrowseId {
        override val mediaId: String get() = COMPOSERS
    }

    /** One album. Its children are the primary release's tracks. */
    data class Album(
        val albumId: String,
    ) : BrowseId {
        override val mediaId: String get() = "$ALBUM_PREFIX$albumId"
    }

    /** One playable track: the release it belongs to and the track's own id,
     *  the pair [play_release][uniffi.bae_bridge.AppHandle.playRelease] takes to
     *  start the release at that track. A client saves this id, so it names the
     *  track itself: a later edit to the tracklist cannot point it elsewhere. */
    data class Track(
        val releaseId: String,
        val trackId: String,
    ) : BrowseId {
        override val mediaId: String get() = "$TRACK_PREFIX${releaseId.length}:$releaseId$trackId"
    }

    /** One composer. Its children are the composer's works and credited albums. */
    data class Composer(
        val artistId: String,
    ) : BrowseId {
        override val mediaId: String get() = "$COMPOSER_PREFIX$artistId"
    }

    /** One work. Its children are child works and the work's releases. */
    data class Work(
        val workId: String,
    ) : BrowseId {
        override val mediaId: String get() = "$WORK_PREFIX$workId"
    }

    companion object {
        const val ROOT = "root"
        const val ALBUMS = "albums"
        const val COMPOSERS = "composers"
        private const val ALBUM_PREFIX = "album:"
        private const val TRACK_PREFIX = "track:"
        private const val COMPOSER_PREFIX = "composer:"
        private const val WORK_PREFIX = "work:"

        /** Parse a client-supplied media id, or null when it names no node. */
        fun parse(mediaId: String): BrowseId? =
            when {
                mediaId == ROOT -> Root
                mediaId == ALBUMS -> Albums
                mediaId == COMPOSERS -> Composers
                mediaId.startsWith(ALBUM_PREFIX) -> Album(mediaId.removePrefix(ALBUM_PREFIX))
                mediaId.startsWith(COMPOSER_PREFIX) -> Composer(mediaId.removePrefix(COMPOSER_PREFIX))
                mediaId.startsWith(WORK_PREFIX) -> Work(mediaId.removePrefix(WORK_PREFIX))
                mediaId.startsWith(TRACK_PREFIX) -> parseTrack(mediaId.removePrefix(TRACK_PREFIX))
                else -> null
            }

        private fun parseTrack(payload: String): Track? {
            val releaseIdLength = payload.substringBefore(':').toIntOrNull()
            val ids = payload.substringAfter(':', missingDelimiterValue = "")
            return if (releaseIdLength != null && releaseIdLength > 0 && ids.length > releaseIdLength) {
                Track(ids.take(releaseIdLength), ids.drop(releaseIdLength))
            } else {
                null
            }
        }
    }
}

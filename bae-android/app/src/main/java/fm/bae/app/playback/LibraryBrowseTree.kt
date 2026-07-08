package fm.bae.app.playback

import android.net.Uri
import androidx.annotation.VisibleForTesting
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import fm.bae.app.BaeLogger
import fm.bae.app.data.Library
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeWorkSummary

private const val TAG = "bae.LibraryBrowseTree"
private val logger = BaeLogger(TAG)

/**
 * The two top-level category names the root exposes. Resolved by the caller
 * through the platform string catalog (the tree has no locale), so the tree
 * builds every node's user-visible text without touching resources itself.
 */
internal data class BrowseLabels(
    val albums: String,
    val composers: String,
)

/**
 * Builds the media-browse tree Android Auto / Bluetooth head units navigate,
 * served from the same paged library reads the in-app browser uses ([Library]).
 * The shape mirrors the app's browser: root → Albums / Composers; an album
 * drills to its primary release's tracks; a composer drills to its works and
 * credited albums, and a work to its child works and releases.
 *
 * Reads are `suspend` and paged: the top-level album and composer lists page
 * straight from the database honoring the browser's requested page window,
 * never loading the whole library. Detail-derived child lists (an album's
 * tracks, a composer's works) come from a single bounded detail read and are
 * sliced to the requested window.
 */
internal class LibraryBrowseTree(
    private val library: Library,
    /** The category labels, resolved through the platform string catalog on
     *  demand (not at construction) so a browse request picks up the current
     *  locale and the service does no resource work until a client browses. */
    private val labels: () -> BrowseLabels,
    /** Maps a cover-image id (a release/composer/work cover) to the content URI
     *  the browse client fetches its bytes from — the same bytes the bridge's
     *  `fetchCoverImageBytes` serves. */
    private val artworkUri: (coverId: String) -> Uri,
) {
    /** The tree root. Its [children] are the top-level categories. */
    fun root(): MediaItem =
        browsableItem(
            id = BrowseId.Root,
            title = ROOT_TITLE,
            coverId = null,
            mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
        )

    /**
     * Children of [parentId] for the requested page window, or null when the id
     * names no node in the tree (the caller reports a bad-id error). An empty
     * list is a valid answer — a node that legitimately has no children.
     */
    suspend fun children(
        parentId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem>? =
        when (val id = BrowseId.parse(parentId)) {
            null -> null
            BrowseId.Root -> {
                val labels = labels()
                paginate(
                    listOf(
                        browsableItem(BrowseId.Albums, labels.albums, null, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS),
                        browsableItem(BrowseId.Composers, labels.composers, null, MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS),
                    ),
                    page,
                    pageSize,
                )
            }

            BrowseId.Albums -> albumListChildren(page, pageSize)
            BrowseId.Composers -> composerListChildren(page, pageSize)
            is BrowseId.Album -> albumTrackChildren(id.albumId, page, pageSize)
            is BrowseId.Composer -> composerNodeChildren(id.artistId, page, pageSize)
            is BrowseId.Work -> workNodeChildren(id.workId, page, pageSize)
            is BrowseId.Track -> {
                // A track is a leaf (playable, not browsable); a browse client
                // should never ask for its children.
                logger.debug("children requested for playable track ${id.releaseId}#${id.index}; none")
                emptyList()
            }
        }

    /** The single node [mediaId] names, or null when it names none. */
    suspend fun item(mediaId: String): MediaItem? =
        when (val id = BrowseId.parse(mediaId)) {
            null -> null
            BrowseId.Root -> root()
            BrowseId.Albums ->
                browsableItem(BrowseId.Albums, labels().albums, null, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS)

            BrowseId.Composers ->
                browsableItem(BrowseId.Composers, labels().composers, null, MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS)

            is BrowseId.Album -> {
                val detail = library.albumDetail(id.albumId)
                albumItem(detail.album.id, detail.album.title, detail.album.cover?.id)
            }

            is BrowseId.Composer -> {
                val detail = library.composerDetail(id.artistId) ?: return null
                composerItem(detail.composer)
            }

            is BrowseId.Work -> {
                val detail = library.workDetail(id.workId) ?: return null
                workItem(detail.work)
            }

            is BrowseId.Track -> {
                val release = library.releaseDetail(id.releaseId) ?: return null
                val track = flatTracks(release).getOrNull(id.index) ?: return null
                trackItem(release, track, id.index)
            }
        }

    /**
     * Search results for the head unit's search screen, as browsable album
     * nodes (the client drills into one to play its tracks). Sliced to the
     * requested page window.
     */
    suspend fun search(
        query: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val albums = library.search(query).albums.map { albumItem(it.id, it.title, it.cover?.id) }
        return paginate(albums, page, pageSize)
    }

    /**
     * The track a spoken "play X" should start, or null when the search finds
     * nothing playable. Prefers a matching track (started in its primary
     * release); falls back to the top album's first track.
     */
    suspend fun searchTopPlayable(query: String): BrowseId.Track? {
        val results = library.search(query)
        results.tracks.firstOrNull()?.let { track ->
            val release = primaryRelease(library.albumDetail(track.albumId)) ?: return null
            val index = flatTracks(release).indexOfFirst { it.id == track.id }
            if (index < 0) {
                logger.debug("spoken-match track ${track.id} not in primary release ${release.id}; starting the release")
            }
            return BrowseId.Track(release.id, index.coerceAtLeast(0))
        }
        results.albums.firstOrNull()?.let { album ->
            val release = primaryRelease(library.albumDetail(album.id)) ?: return null
            return BrowseId.Track(release.id, 0)
        }
        return null
    }

    private suspend fun albumListChildren(
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val albums = library.albumPage(listOf(ALBUM_SORT), offsetOf(page, pageSize), limitOf(pageSize))
        return albums.map { albumItem(it.id, it.title, it.cover?.id) }
    }

    private suspend fun composerListChildren(
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val composers = library.composerPage(COMPOSER_SORT, offsetOf(page, pageSize), limitOf(pageSize))
        return composers.map { composerItem(it) }
    }

    private suspend fun albumTrackChildren(
        albumId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val detail = library.albumDetail(albumId)
        val release = primaryRelease(detail) ?: return emptyList()
        val items = flatTracks(release).mapIndexed { index, track -> trackItem(release, track, index) }
        return paginate(items, page, pageSize)
    }

    private suspend fun composerNodeChildren(
        artistId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val detail = library.composerDetail(artistId)
        if (detail == null) {
            logger.warning("composer $artistId has no detail; no browse children")
            return emptyList()
        }
        val works = detail.workGroups.flatMap { group -> listOfNotNull(group.parent) + group.works }
        val workItems = works.map { workItem(it) }
        val creditAlbums = detail.unlinkedReleaseRoles.map { albumItem(it.albumId, it.albumTitle, null) }
        return paginate(workItems + creditAlbums, page, pageSize)
    }

    private suspend fun workNodeChildren(
        workId: String,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val detail = library.workDetail(workId)
        if (detail == null) {
            logger.warning("work $workId has no detail; no browse children")
            return emptyList()
        }
        val childWorkItems = detail.childWorks.map { workItem(it) }
        val releaseAlbums = detail.releases.map { albumItem(it.albumId, it.albumTitle, it.cover?.id) }
        return paginate(childWorkItems + releaseAlbums, page, pageSize)
    }

    // ── Node builders ────────────────────────────────────────────────────

    private fun albumItem(
        albumId: String,
        title: String,
        coverId: String?,
    ): MediaItem = browsableItem(BrowseId.Album(albumId), title, coverId, MediaMetadata.MEDIA_TYPE_ALBUM)

    private fun composerItem(composer: BridgeComposerSummary): MediaItem =
        browsableItem(BrowseId.Composer(composer.artistId), composer.name, composer.image?.id, MediaMetadata.MEDIA_TYPE_ARTIST)

    private fun workItem(work: BridgeWorkSummary): MediaItem =
        browsableItem(
            id = BrowseId.Work(work.workId),
            title = work.title,
            coverId = work.representativeCover?.id,
            mediaType = MediaMetadata.MEDIA_TYPE_FOLDER_MIXED,
            subtitle = work.composerNames,
        )

    private fun browsableItem(
        id: BrowseId,
        title: String,
        coverId: String?,
        mediaType: Int,
        subtitle: String? = null,
    ): MediaItem {
        val metadata =
            baseMetadata(title, coverId)
                .setSubtitle(subtitle)
                .setIsBrowsable(true)
                .setIsPlayable(false)
                .setMediaType(mediaType)
                .build()
        return MediaItem.Builder().setMediaId(id.mediaId).setMediaMetadata(metadata).build()
    }

    private fun trackItem(
        release: BridgeRelease,
        track: BridgeTrack,
        index: Int,
    ): MediaItem {
        val metadata =
            baseMetadata(track.title, release.cover?.id)
                .setArtist(track.artistNames)
                .setDurationMs(track.durationMs)
                .setIsBrowsable(false)
                .setIsPlayable(true)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .build()
        return MediaItem
            .Builder()
            .setMediaId(BrowseId.Track(release.id, index).mediaId)
            .setMediaMetadata(metadata)
            .build()
    }

    private fun baseMetadata(
        title: String,
        coverId: String?,
    ): MediaMetadata.Builder {
        val builder = MediaMetadata.Builder().setTitle(title)
        coverId?.let { builder.setArtworkUri(artworkUri(it)) }
        return builder
    }

    private fun primaryRelease(detail: uniffi.bae_bridge.BridgeAlbumDetail): BridgeRelease? =
        detail.releases.firstOrNull { it.id == detail.album.primaryReleaseId }
            ?: detail.releases.firstOrNull()

    // The release-wide flat track order (across track groups) — the same
    // flattening the album-detail screen taps into, so a track's position here
    // is the start index `play_release` expects.
    private fun flatTracks(release: BridgeRelease): List<BridgeTrack> = release.trackGroups.flatMap { it.tracks }

    private companion object {
        const val ROOT_TITLE = "bae"

        // The browser's default album and composer orderings — newest albums
        // first, composers by name — matching the in-app browser's defaults.
        val ALBUM_SORT = BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
        val COMPOSER_SORT = BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)

        // Upper bound on how many rows one page read pulls. A browse client that
        // subscribes without paging asks for Int.MAX_VALUE; cap it so a browse
        // read never pulls the whole library.
        const val MAX_PAGE_SIZE = 500

        fun limitOf(pageSize: Int): ULong = pageSize.coerceIn(1, MAX_PAGE_SIZE).toULong()

        fun offsetOf(
            page: Int,
            pageSize: Int,
        ): ULong = (page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0)).toULong()

        @VisibleForTesting
        fun paginate(
            items: List<MediaItem>,
            page: Int,
            pageSize: Int,
        ): List<MediaItem> {
            val limit = pageSize.coerceIn(1, MAX_PAGE_SIZE)
            val start = (page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0))
            if (start >= items.size) return emptyList()
            return items.drop(start.toInt()).take(limit)
        }
    }
}

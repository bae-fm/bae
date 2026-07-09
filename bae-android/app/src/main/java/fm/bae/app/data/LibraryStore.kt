package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.bae_bridge.BridgeAlbumDetail

/**
 * Normalized cache of library entities, populated by invalidation-triggered
 * detail reads and on-demand detail loads. The bridge types are the display
 * types, so they're stored directly — no native mirror.
 *
 * ## Slices
 * - [albumDetails] — fat [BridgeAlbumDetail] (album + releases) keyed by album
 *   id. The album-detail screen reads this; it's also the source of the album's
 *   release list and cover.
 *
 * ## Generation
 * The live grid pages albums straight from the database via [Library], so the
 * store does not hold the grid's rows. Instead [generation] bumps on album-list
 * invalidation. The grid observes it and re-queries `getAlbumCount` /
 * `getAlbumPage`, so sync streaming albums in over time shows up live.
 */
class LibraryStore {
    private val _albumDetails = MutableStateFlow<Map<String, BridgeAlbumDetail>>(emptyMap())
    val albumDetails: StateFlow<Map<String, BridgeAlbumDetail>> = _albumDetails.asStateFlow()

    private val _generation = MutableStateFlow(0L)
    val generation: StateFlow<Long> = _generation.asStateFlow()

    private val _composerGeneration = MutableStateFlow(0L)
    val composerGeneration: StateFlow<Long> = _composerGeneration.asStateFlow()

    private val _artistGeneration = MutableStateFlow(0L)
    val artistGeneration: StateFlow<Long> = _artistGeneration.asStateFlow()

    fun albumDetail(albumId: String): BridgeAlbumDetail? = _albumDetails.value[albumId]

    /**
     * Seed the detail slice from an on-demand DB load when no event has yet
     * populated it. Does not bump [generation] — this reflects existing
     * library shape, not a list-shape invalidation. If a later invalidation
     * already interned a detail for this album, the seed is dropped so the
     * invalidation read wins.
     */
    fun seedAlbumDetail(album: BridgeAlbumDetail) {
        _albumDetails.update { details ->
            if (details.containsKey(album.album.id)) {
                details
            } else {
                details + (album.album.id to album)
            }
        }
    }

    fun invalidateAlbumList() {
        _generation.update { it + 1 }
    }

    fun invalidateComposerList() {
        _composerGeneration.update { it + 1 }
    }

    fun invalidateArtistList() {
        _artistGeneration.update { it + 1 }
    }

    fun replaceAlbumDetail(album: BridgeAlbumDetail) {
        _albumDetails.update { it + (album.album.id to album) }
    }
}

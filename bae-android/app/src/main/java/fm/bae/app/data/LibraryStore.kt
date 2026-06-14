package fm.bae.app.data

import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeRelease

private const val TAG = "bae.LibraryStore"

/**
 * Normalized cache of library entities, populated by sync-driven library
 * events and on-demand detail loads. The bridge types are the display types,
 * so they're stored directly — no native mirror.
 *
 * ## Slices
 * - [albumDetails] — fat [BridgeAlbumDetail] (album + releases) keyed by album
 *   id. The album-detail screen reads this; it's also the source of the album's
 *   release list and cover.
 *
 * ## Generation
 * The live grid pages albums straight from the database via [Library], so the
 * store does not hold the grid's rows. Instead [generation] bumps whenever the
 * library's shape changes (album/release add/update/remove). The grid observes
 * it and re-queries `getAlbumCount` / `getAlbumPage`, so sync streaming albums
 * in over time shows up live. This mirrors the desktop's `libraryShapeSubject`.
 */
class LibraryStore {
    private val _albumDetails = MutableStateFlow<Map<String, BridgeAlbumDetail>>(emptyMap())
    val albumDetails: StateFlow<Map<String, BridgeAlbumDetail>> = _albumDetails.asStateFlow()

    private val _generation = MutableStateFlow(0L)
    val generation: StateFlow<Long> = _generation.asStateFlow()

    fun albumDetail(albumId: String): BridgeAlbumDetail? = _albumDetails.value[albumId]

    /**
     * Seed the detail slice from an on-demand DB load when no event has yet
     * populated it. Does not bump [generation] — this reflects existing
     * library shape, not a change to it. If a later event already interned a
     * detail for this album, the seed is dropped so the event's data wins.
     */
    fun seedAlbumDetail(album: BridgeAlbumDetail) {
        _albumDetails.update { details ->
            if (details.containsKey(album.album.id)) details
            else details + (album.album.id to album)
        }
    }

    private fun bumpGeneration() {
        _generation.update { it + 1 }
    }

    // ── Album event handlers ──────────────────────────────────────────────

    fun handleAlbumAdded(album: BridgeAlbumDetail) {
        _albumDetails.update { it + (album.album.id to album) }
        bumpGeneration()
    }

    fun handleAlbumUpdated(album: BridgeAlbumDetail) {
        _albumDetails.update { it + (album.album.id to album) }
        bumpGeneration()
    }

    fun handleAlbumRemoved(albumId: String) {
        _albumDetails.update { it - albumId }
        bumpGeneration()
    }

    // ── Release event handlers ────────────────────────────────────────────
    //
    // Release events carry the parent album (or its id) plus the affected
    // release. The detail slice is keyed by album, so each handler reads the
    // current detail, applies the release change, and writes the album back.
    // These do not bump [generation]: release-level changes don't add, remove,
    // or reorder album-grid rows. The detail screen observes [albumDetails]
    // directly, so it re-renders on these without a grid refresh.

    fun handleReleaseAdded(album: BridgeAlbum, release: BridgeRelease) {
        _albumDetails.update { details ->
            val existing = details[album.id]
            val releases = (existing?.releases.orEmpty().filter { it.id != release.id }) + release
            details + (album.id to BridgeAlbumDetail(album = album, releases = releases))
        }
    }

    fun handleReleaseUpdated(albumId: String, release: BridgeRelease) {
        _albumDetails.update { details ->
            val existing = details[albumId] ?: run {
                Log.w(TAG, "ReleaseUpdated for un-interned album $albumId (release ${release.id}); dropping")
                return@update details
            }
            val releases = existing.releases.map { if (it.id == release.id) release else it }
            details + (albumId to existing.copy(releases = releases))
        }
    }

    fun handleReleaseRemoved(albumId: String, releaseId: String, album: BridgeAlbum?) {
        // album is null when the album was removed with its last release;
        // AlbumRemoved already dropped the detail, so there's nothing to do.
        if (album == null) return
        _albumDetails.update { details ->
            val existing = details[albumId] ?: run {
                Log.w(TAG, "ReleaseRemoved for un-interned album $albumId (release $releaseId); dropping")
                return@update details
            }
            val releases = existing.releases.filter { it.id != releaseId }
            // Use the event's post-removal album so releaseIds reflects the
            // authoritative DB-ordered list, not the stale interned one.
            details + (albumId to BridgeAlbumDetail(album = album, releases = releases))
        }
    }
}

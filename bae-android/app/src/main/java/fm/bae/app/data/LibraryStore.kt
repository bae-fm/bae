package fm.bae.app.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import uniffi.bae_bridge.BridgeAlbumDetail

/**
 * Normalized cache of library entities populated by on-demand detail loads.
 * The bridge types are the display
 * types, so they're stored directly — no native mirror.
 *
 * ## Slices
 * - [albumDetails] — fat [BridgeAlbumDetail] (album + releases) keyed by album
 *   id. The album-detail screen reads this; it's also the source of the album's
 *   release list and cover.
 *
 */
class LibraryStore {
    private val _albumDetails = MutableStateFlow<Map<String, BridgeAlbumDetail>>(emptyMap())
    val albumDetails: StateFlow<Map<String, BridgeAlbumDetail>> = _albumDetails.asStateFlow()

    fun albumDetail(albumId: String): BridgeAlbumDetail? = _albumDetails.value[albumId]

    fun applyAlbumDetail(albumId: String, album: BridgeAlbumDetail?) {
        _albumDetails.update { details ->
            if (album == null) {
                details - albumId
            } else {
                details + (album.album.id to album)
            }
        }
    }

}

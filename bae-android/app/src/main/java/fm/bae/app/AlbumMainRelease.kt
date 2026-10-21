package fm.bae.app

import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeRelease

/**
 * The release the album names as its main one. Core names one of the album's
 * own releases, so null is a broken guarantee: the caller reports it as a bug
 * ([MissingMainRelease]) rather than standing another release in for it.
 */
internal fun BridgeAlbumDetail.mainRelease(): BridgeRelease? = releases.firstOrNull { it.id == album.primaryReleaseId }

/** An album whose named main release isn't among its releases. */
internal class MissingMainRelease(
    detail: BridgeAlbumDetail,
) : IllegalStateException(
        "album ${detail.album.id} names main release ${detail.album.primaryReleaseId}, " +
            "which isn't among its releases ${detail.releases.map { it.id }}",
    )

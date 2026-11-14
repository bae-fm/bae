package fm.bae.app

import uniffi.bae_bridge.BridgeAlbumDetail

/**
 * An album with no releases. Core lists an album's releases with its main one first, and an album
 * has at least one, so the first release is the main one and none is a broken guarantee: the
 * caller reports it as a bug rather than standing something else in for it.
 */
internal class MissingMainRelease(
    detail: BridgeAlbumDetail,
) : IllegalStateException("album ${detail.album.id} has no releases, so no main one")

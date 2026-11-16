package fm.bae.app

import uniffi.bae_bridge.BridgeAlbumDetail

/**
 * An album with no releases. Core lists an album's releases with its main one first, and an album
 * has at least one, so the first release is the main one. An album with no releases breaks that
 * guarantee, so the caller reports a bug instead of showing another release in its place.
 */
internal class MissingMainRelease(
    detail: BridgeAlbumDetail,
) : IllegalStateException("album ${detail.album.id} has no releases, so it has no main release")

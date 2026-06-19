package fm.bae.app

import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDiscogsTokenStatus
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeTrackGroup

/**
 * Plain `Bridge*` data-class constructors for the JVM unit tests. These build
 * the uniffi types directly — no native lib, no `AppHandle` — so the data layer
 * can be exercised without the Rust runtime.
 */
object BridgeFixtures {
    fun album(
        id: String,
        title: String = "Album Title",
        primaryReleaseId: String = "rel-$id",
        releaseIds: List<String> = listOf(primaryReleaseId),
    ): BridgeAlbum = BridgeAlbum(
        id = id,
        title = title,
        year = null,
        isCompilation = false,
        artistNames = "Artist Name",
        releaseIds = releaseIds,
        primaryReleaseId = primaryReleaseId,
        coverPath = null,
    )

    fun release(
        id: String,
        albumId: String,
        trackGroups: List<BridgeTrackGroup> = emptyList(),
        galleryItems: List<BridgeGalleryItem> = emptyList(),
    ): BridgeRelease = BridgeRelease(
        id = id,
        albumId = albumId,
        displayName = "Release",
        releaseName = null,
        year = null,
        format = null,
        label = null,
        catalogNumber = null,
        country = null,
        storageState = BridgeReleaseStorageState.UNMANAGED,
        storageActions = emptyList(),
        tracks = trackGroups.flatMap { it.tracks },
        trackGroups = trackGroups,
        files = emptyList(),
        imageFiles = emptyList(),
        galleryItems = galleryItems,
        totalDurationMs = 0,
        fileCount = 0,
        totalSize = 0,
        coverPath = null,
    )

    fun albumDetail(
        album: BridgeAlbum,
        releases: List<BridgeRelease> = listOf(release(id = album.primaryReleaseId, albumId = album.id)),
    ): BridgeAlbumDetail = BridgeAlbumDetail(album = album, releases = releases)

    fun config(libraryId: String = "lib-1"): BridgeConfig = BridgeConfig(
        libraryId = libraryId,
        libraryName = "bae Library",
        libraryPath = "/tmp/lib",
        encryptionKeyStored = false,
        encryptionKeyFingerprint = null,
        discogsTokenStatus = BridgeDiscogsTokenStatus.NOT_CONFIGURED,
        discogsUsable = false,
        sync = null,
    )
}

package fm.bae.app

import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDiscogsTokenStatus
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeMcpConfig
import uniffi.bae_bridge.BridgeOutboxSnapshot
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeSaveBitDepth
import uniffi.bae_bridge.BridgeSaveCodec
import uniffi.bae_bridge.BridgeSaveFilenameToken
import uniffi.bae_bridge.BridgeSavePregapPlacement
import uniffi.bae_bridge.BridgeSavePreset
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeUploadProgress
import uniffi.bae_bridge.BridgeWorkSummary

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
    ): BridgeAlbum =
        BridgeAlbum(
            id = id,
            title = title,
            year = null,
            isCompilation = false,
            artistNames = "Artist Name",
            releaseIds = releaseIds,
            primaryReleaseId = primaryReleaseId,
            cover = null,
        )

    fun release(
        id: String,
        albumId: String,
        trackGroups: List<BridgeTrackGroup> = emptyList(),
        galleryItems: List<BridgeGalleryItem> = emptyList(),
    ): BridgeRelease =
        BridgeRelease(
            id = id,
            albumId = albumId,
            displayName = "Release",
            year = null,
            format = null,
            label = null,
            catalogNumber = null,
            country = null,
            storageState = BridgeReleaseStorageState.LOCAL,
            pinned = false,
            storageActions = emptyList(),
            transferAction = null,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            files = emptyList(),
            imageFiles = emptyList(),
            galleryItems = galleryItems,
            totalDuration = null,
            fileCount = 0,
            totalSize = 0,
            cover = null,
        )

    fun albumDetail(
        album: BridgeAlbum,
        releases: List<BridgeRelease> = listOf(release(id = album.primaryReleaseId, albumId = album.id)),
    ): BridgeAlbumDetail = BridgeAlbumDetail(album = album, releases = releases)

    fun albumSearchResult(
        id: String = "alb-1",
        title: String = "Album Title",
    ): BridgeAlbumSearchResult =
        BridgeAlbumSearchResult(
            id = id,
            title = title,
            year = null,
            artistName = "Artist Name",
            cover = null,
        )

    fun trackSearchResult(
        id: String = "trk-1",
        albumId: String = "alb-1",
    ): BridgeTrackSearchResult =
        BridgeTrackSearchResult(
            id = id,
            title = "Track Title",
            durationMs = null,
            albumId = albumId,
            albumTitle = "Album Title",
            artistName = "Artist Name",
            cover = null,
        )

    fun artistSummary(
        artistId: String = "artist-1",
        name: String = "Artist Name",
    ): BridgeArtistSummary =
        BridgeArtistSummary(
            artistId = artistId,
            name = name,
            albumCount = 1L,
            image = null,
        )

    fun composerSummary(
        artistId: String = "artist-1",
        name: String = "Composer Name",
    ): BridgeComposerSummary =
        BridgeComposerSummary(
            artistId = artistId,
            name = name,
            sortName = null,
            workCount = 1L,
            linkedReleaseCount = 1L,
            unlinkedCreditCount = 0L,
            image = null,
        )

    fun workSummary(
        workId: String = "work-1",
        title: String = "Work Title",
    ): BridgeWorkSummary =
        BridgeWorkSummary(
            workId = workId,
            title = title,
            disambiguation = null,
            workType = null,
            parentWorkId = null,
            composerNames = "Composer Name",
            linkedReleaseCount = 1L,
            representativeReleaseId = "rel-1",
            representativeCover = null,
        )

    fun searchResults(
        albums: List<BridgeAlbumSearchResult> = emptyList(),
        artists: List<BridgeArtistSummary> = emptyList(),
        tracks: List<BridgeTrackSearchResult> = emptyList(),
        composers: List<BridgeComposerSummary> = emptyList(),
        works: List<BridgeWorkSummary> = emptyList(),
    ): BridgeSearchResults =
        BridgeSearchResults(
            albums = albums,
            artists = artists,
            tracks = tracks,
            composers = composers,
            works = works,
        )

    fun downloadOp(
        releaseId: String,
        state: BridgeDownloadState,
        title: String = "Album Title",
        fileCount: Long = 10,
        totalSize: Long = 1_000,
    ): BridgeDownloadOp =
        BridgeDownloadOp(
            releaseId = releaseId,
            title = title,
            fileCount = fileCount,
            totalSize = totalSize,
            createdAt = 0,
            state = state,
        )

    fun downloadSnapshot(
        queued: UInt = 0u,
        active: UInt = 0u,
        failed: UInt = 0u,
        paused: Boolean = false,
        downloads: List<BridgeDownloadOp> = emptyList(),
    ): BridgeDownloadSnapshot =
        BridgeDownloadSnapshot(
            downloads = downloads,
            total =
                BridgeDownloadProgress(
                    queued = queued,
                    active = active,
                    failed = failed,
                ),
            summaryParts = emptyList(),
            paused = paused,
        )

    fun outboxSnapshot(paused: Boolean = false): BridgeOutboxSnapshot =
        BridgeOutboxSnapshot(
            uploadGroups = emptyList(),
            deletes = emptyList(),
            perRelease = emptyMap(),
            total =
                BridgeUploadProgress(
                    queued = 0u,
                    active = 0u,
                    failed = 0u,
                    bytesDone = 0uL,
                    bytesTotal = 0uL,
                    activity = null,
                ),
            pendingDeletes = 0u,
            summaryParts = emptyList(),
            paused = paused,
            throughputBps = 0uL,
            etaSeconds = null,
        )

    fun library(
        id: String,
        name: String = "Library $id",
    ): BridgeLibrary =
        BridgeLibrary(
            id = id,
            name = name,
            path = "/tmp/$id",
            cloudProvider = null,
            isActive = false,
            error = null,
        )

    fun config(libraryId: String = "lib-1"): BridgeConfig =
        BridgeConfig(
            libraryId = libraryId,
            libraryName = "bae Library",
            libraryPath = "/tmp/lib",
            encryptionKeyStored = false,
            encryptionKeyFingerprint = null,
            pauseBetweenSides = false,
            maxConcurrentUploads = 3u,
            maxConcurrentDownloads = 3u,
            showRemainingTime = false,
            libraryFullWidth = false,
            savePresets =
                listOf(
                    BridgeSavePreset(
                        id = "flac",
                        name = "FLAC",
                        codec = BridgeSaveCodec.Flac(bitDepth = BridgeSaveBitDepth.SOURCE),
                        extension = "flac",
                        filenameTokens =
                            listOf(
                                BridgeSaveFilenameToken.TRACK_NUMBER,
                                BridgeSaveFilenameToken.TITLE,
                            ),
                        pregapPlacement =
                            BridgeSavePregapPlacement.APPEND_TO_PREVIOUS_EXCEPT_HTOA,
                        appliesToTrack = true,
                        appliesToRelease = true,
                        embedCover = true,
                    )
                ),
            defaultTrackSavePreset = "flac",
            defaultReleaseSavePreset = "flac",
            mcp = BridgeMcpConfig(enabled = false, port = 47777u),
            discogsTokenStatus = BridgeDiscogsTokenStatus.NOT_CONFIGURED,
            discogsUsable = false,
            sync = null,
        )
}

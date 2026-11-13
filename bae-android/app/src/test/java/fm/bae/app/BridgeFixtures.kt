package fm.bae.app

import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeCacheLimit
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeCreditLine
import uniffi.bae_bridge.BridgeDeviceStorage
import uniffi.bae_bridge.BridgeDiscogsTokenStatus
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadKind
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeImportDestination
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeLookupCatalog
import uniffi.bae_bridge.BridgeLookupCatalogSetting
import uniffi.bae_bridge.BridgeMcpConfig
import uniffi.bae_bridge.BridgePlaylistCover
import uniffi.bae_bridge.BridgePlaylistListing
import uniffi.bae_bridge.BridgePlaylistOwner
import uniffi.bae_bridge.BridgePlaylistSummary
import uniffi.bae_bridge.BridgePressingFacts
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeQueuedRelease
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeRendererKind
import uniffi.bae_bridge.BridgeSaveBitDepth
import uniffi.bae_bridge.BridgeSaveCodec
import uniffi.bae_bridge.BridgeSaveExtras
import uniffi.bae_bridge.BridgeSaveFilenamePiece
import uniffi.bae_bridge.BridgeSaveFilenameValue
import uniffi.bae_bridge.BridgeSaveFolderPiece
import uniffi.bae_bridge.BridgeSaveFolderValue
import uniffi.bae_bridge.BridgeSavePreset
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.BridgeSidePauseCountdown
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeSourceAvailability
import uniffi.bae_bridge.BridgeStorageKind
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStoragePart
import uniffi.bae_bridge.BridgeSubsonicConfig
import uniffi.bae_bridge.BridgeSubsonicNetwork
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSearchResult
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
            artistLine = creditLineNamed("Artist Name"),
            releaseIds = releaseIds,
            primaryReleaseId = primaryReleaseId,
            cover = null,
        )

    fun release(
        id: String,
        albumId: String,
        trackGroups: List<BridgeTrackGroup> = emptyList(),
        sourceAudio: BridgeSourceAudioSummary? = null,
        galleryItems: List<BridgeGalleryItem> = emptyList(),
    ): BridgeRelease =
        BridgeRelease(
            id = id,
            albumId = albumId,
            name = BridgeReleaseName.Numbered(1),
            distinctYear = null,
            labels = emptyList(),
            facts =
                BridgePressingFacts(
                    area = null,
                    media = emptyList(),
                    status = null,
                    packaging = null,
                    discogsDetails = emptyList(),
                ),
            pressingSummary = emptyList(),
            mediaTerms = emptyList(),
            pressingDetails = emptyList(),
            storageState = BridgeReleaseStorageState.OnOneDevice,
            storageActions = emptyList(),
            upload = null,
            transitioning = false,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            sourceAudio = sourceAudio,
            otherFiles = emptyList(),
            imageFiles = emptyList(),
            coverFiles = emptyList(),
            galleryItems = galleryItems,
            playability = BridgeReleasePlayability.Playable,
            makeLocal = null,
            downloadStatus = null,
            records = emptyList(),
            totalDuration = null,
            fileCount = 0,
            totalSize = 0UL,
            cover = null,
        )

    fun albumDetail(
        album: BridgeAlbum,
        releases: List<BridgeRelease> = listOf(release(id = album.primaryReleaseId, albumId = album.id)),
    ): BridgeAlbumDetail =
        BridgeAlbumDetail(
            album = album,
            releases = releases,
            rating = BridgeAlbumRating(canRate = false, mine = null, raters = emptyList()),
        )

    fun albumSearchResult(
        id: String = "alb-1",
        title: String = "Album Title",
    ): BridgeAlbumSearchResult =
        BridgeAlbumSearchResult(
            id = id,
            title = title,
            year = null,
            artistLine = creditLineNamed("Artist Name"),
            cover = null,
        )

    fun trackSearchResult(
        id: String = "trk-1",
        releaseId: String = "rel-1",
        albumId: String = "alb-1",
    ): BridgeTrackSearchResult =
        BridgeTrackSearchResult(
            id = id,
            title = "Track Title",
            durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
            releaseId = releaseId,
            albumId = albumId,
            albumTitle = "Album Title",
            artistLine = creditLineNamed("Artist Name"),
            cover = null,
        )

    fun artistSummary(
        artistId: String = "artist-1",
        name: String = "Artist Name",
    ): BridgeArtistSummary =
        BridgeArtistSummary(
            artistId = artistId,
            name = name,
            countLine = BridgeCountLabel(key = "core.library.album_count", count = 1u),
            image = null,
        )

    fun playlistSummary(
        id: String = "playlist-1",
        name: String = "Late night",
        sharing: BridgeSharing = BridgeSharing.PRIVATE,
    ): BridgePlaylistSummary =
        BridgePlaylistSummary(
            id = id,
            name = name,
            sharing = sharing,
            owner = BridgePlaylistOwner(person = "person-1", name = "Mara", avatarVersion = null),
            yours = true,
            trackCount = BridgeCountLabel(key = "core.library.track_count", count = 12u),
            totalDuration = null,
            cover = BridgePlaylistCover.Single(cover = null),
        )

    fun playlistListing(
        listed: List<BridgePlaylistSummary> = listOf(playlistSummary()),
        canCreate: Boolean = true,
    ): BridgePlaylistListing = BridgePlaylistListing(listed = listed, addTargets = listed, canCreate = canCreate)

    fun composerSummary(
        artistId: String = "artist-1",
        name: String = "Composer Name",
    ): BridgeComposerSummary =
        BridgeComposerSummary(
            artistId = artistId,
            name = name,
            sortName = null,
            countLine = BridgeCountLabel(key = "core.library.work_count", count = 1u),
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
        fileCount: ULong = 10UL,
        totalSize: ULong = 1_000UL,
    ): BridgeDownloadOp =
        BridgeDownloadOp(
            releaseId = releaseId,
            kind = BridgeDownloadKind.PIN,
            release = BridgeQueuedRelease(title = title, fileCount = fileCount, totalSize = totalSize),
            createdAt = 0,
            state = state,
        )

    fun downloadCounts(
        queued: UInt = 0u,
        active: UInt = 0u,
        failed: UInt = 0u,
        wait: BridgeQueueWait? = null,
    ): BridgeDownloadCounts =
        BridgeDownloadCounts(
            total =
                BridgeDownloadProgress(
                    queued = queued,
                    active = active,
                    failed = failed,
                ),
            wait = wait,
        )

    fun castReceiver(
        id: String = "cast-1",
        name: String = "Kitchen",
        kind: BridgeRendererKind = BridgeRendererKind.CAST,
    ): BridgeCastReceiver = BridgeCastReceiver(id = id, name = name, kind = kind, model = null, address = "192.168.1.40")

    fun library(
        id: String,
        name: String = "Library $id",
    ): BridgeLibrary =
        BridgeLibrary(
            id = id,
            name = name,
            path = "/tmp/$id",
            state = BridgeLibraryState.Openable(BridgeStorageLocation.S3("music", "us-east-1", null, null)),
            isActive = false,
        )

    fun config(libraryId: String = "lib-1"): BridgeConfig =
        BridgeConfig(
            libraryId = libraryId,
            libraryPath = "/tmp/lib",
            pauseBetweenSides = false,
            sidePauseCountdown = BridgeSidePauseCountdown.OFF,
            maxConcurrentUploads = 3u,
            maxConcurrentDownloads = 3u,
            maxConcurrentTransfers = 8u,
            identifyAutomatically = true,
            importWhenIdentified = false,
            importDestination = BridgeImportDestination.CLOUD,
            // Every source in core's order, matching this fixture's Discogs
            // state below: MusicBrainz is asked and, as the only source still
            // being asked, cannot be switched off; Discogs holds no key, so its
            // switch cannot be moved either.
            lookupCatalogs =
                listOf(
                    BridgeLookupCatalogSetting(
                        catalog = BridgeLookupCatalog.MUSIC_BRAINZ,
                        availability = BridgeSourceAvailability.ON,
                        canChange = false,
                    ),
                    BridgeLookupCatalogSetting(
                        catalog = BridgeLookupCatalog.DISCOGS,
                        availability = BridgeSourceAvailability.NOT_CONFIGURED,
                        canChange = false,
                    ),
                ),
            hasSourceToSearch = true,
            showRemainingTime = false,
            libraryFullWidth = false,
            askBeforeUnpinning = true,
            savePresets =
                listOf(
                    BridgeSavePreset(
                        id = "flac",
                        name = "FLAC",
                        codec = BridgeSaveCodec.Flac(bitDepth = BridgeSaveBitDepth.SAME_AS_SOURCE),
                    ),
                ),
            filenamePieces =
                listOf(
                    BridgeSaveFilenamePiece.Value(BridgeSaveFilenameValue.TRACK_NUMBER),
                    BridgeSaveFilenamePiece.Text(" "),
                    BridgeSaveFilenamePiece.Value(BridgeSaveFilenameValue.TITLE),
                ),
            filenamePatternError = null,
            folderPieces =
                listOf(
                    BridgeSaveFolderPiece.Value(BridgeSaveFolderValue.ARTIST),
                    BridgeSaveFolderPiece.Text(" - "),
                    BridgeSaveFolderPiece.Value(BridgeSaveFolderValue.ALBUM),
                ),
            folderPatternError = null,
            saveExtras =
                BridgeSaveExtras(
                    embedCover = true,
                    coverFile = true,
                    loudnessTags = true,
                ),
            lastSavePreset = "flac",
            castEnabled = false,
            castTranscodeFormat = BridgeCastTranscodeFormat.MP3,
            mcp = BridgeMcpConfig(enabled = false, port = 47777u),
            discogsTokenStatus = BridgeDiscogsTokenStatus.NOT_CONFIGURED,
            discogsUsable = false,
            subsonic =
                BridgeSubsonicConfig(
                    enabled = false,
                    port = 4533u,
                    username = "",
                    network = BridgeSubsonicNetwork.ThisMachine,
                ),
        )

    /**
     * What a phone keeps: 2.4 GB pinned, 1.2 GB cached, 260 MB of library, and, when [beforeReset] is given, that
     * many bytes a reset kept. The cache keeps up to 20 GB, and the cache can be cleared while it holds bytes.
     */
    fun deviceStorage(beforeReset: ULong? = null): BridgeDeviceStorage {
        val pinned = 2_400_000_000uL
        val cache = 1_200_000_000uL
        val library = 260_000_000uL
        val total = pinned + cache + library + (beforeReset ?: 0uL)
        return BridgeDeviceStorage(
            total = total,
            parts =
                listOfNotNull(
                    BridgeStoragePart(BridgeStorageKind.PINNED, pinned),
                    BridgeStoragePart(BridgeStorageKind.CACHE, cache),
                    BridgeStoragePart(BridgeStorageKind.LIBRARY, library),
                    beforeReset?.let { BridgeStoragePart(BridgeStorageKind.BEFORE_RESET, it) },
                ),
            cache = cache,
            canClearCache = cache > 0uL,
            cacheLimit = cacheLimit(BridgeCacheLimitChoice.GB20, 20),
            cacheLimits =
                listOf(
                    cacheLimit(BridgeCacheLimitChoice.GB1, 1),
                    cacheLimit(BridgeCacheLimitChoice.GB5, 5),
                    cacheLimit(BridgeCacheLimitChoice.GB10, 10),
                    cacheLimit(BridgeCacheLimitChoice.GB20, 20),
                    cacheLimit(BridgeCacheLimitChoice.GB50, 50),
                ),
            beforeReset = beforeReset,
        )
    }

    private fun cacheLimit(
        choice: BridgeCacheLimitChoice,
        gigabytes: Int,
    ) = BridgeCacheLimit(choice, gigabytes.toULong() * 1_000_000_000uL)
}

/** A credit line as plain text, for a test that does not read it in a language. */
fun plainCreditText(line: BridgeCreditLine): String = line.names.joinToString(", ") { it.name }

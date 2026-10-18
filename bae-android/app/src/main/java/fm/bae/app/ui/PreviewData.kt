package fm.bae.app.ui

import fm.bae.app.playback.SidePausePrompt
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDevice
import uniffi.bae_bridge.BridgeDeviceState
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryImageType
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberPublicKeys
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePauseBoundary
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgePressingFacts
import uniffi.bae_bridge.BridgeQueuedRelease
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseArea
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeWorkSummary

/** A member's signing and sealing public keys are each 32 bytes. */
private const val PUBLIC_KEY_BYTES = 32

/**
 * `Bridge*` fixtures for `@Preview`s, in the main source set so previews need no
 * native library or `AppHandle`. Names are generic placeholders, never real
 * artists, albums or tracks.
 */
object PreviewData {
    // Preview track length (3:34) and the same value as a clock, built here
    // rather than through `bridgeClock` because the preview renderer cannot
    // call the native bridge.
    private const val TRACK_DURATION_MS = 214_000L
    private val trackDurationClock =
        BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 34u)

    fun imageRef(id: String = "img-1"): BridgeImageRef =
        BridgeImageRef(
            id = id,
            version = "1",
            imageType = BridgeLibraryImageType.COVER,
        )

    fun album(
        id: String = "alb-1",
        title: String = "Album Title",
        year: Int? = 2020,
        cover: BridgeImageRef? = imageRef(),
    ): BridgeAlbum =
        BridgeAlbum(
            id = id,
            title = title,
            year = year,
            isCompilation = false,
            artistNames = "Artist Name",
            releaseIds = listOf("rel-$id"),
            primaryReleaseId = "rel-$id",
            cover = cover,
        )

    fun track(
        id: String = "trk-1",
        title: String = "Track Title",
        trackNumber: Int? = 1,
    ): BridgeTrack =
        BridgeTrack(
            id = id,
            title = title,
            side = 0,
            trackNumber = trackNumber,
            durationMs = TRACK_DURATION_MS,
            durationClock = trackDurationClock,
            artistNames = "Artist Name",
            displayArtist = null,
            positionText = trackNumber?.toString() ?: "",
        )

    fun trackGroup(
        tracks: List<BridgeTrack> = listOf(track("trk-1", "Track Title", 1), track("trk-2", "Another Track", 2)),
    ): BridgeTrackGroup =
        BridgeTrackGroup(
            side = BridgeTrackSide.Flat,
            headerKey = null,
            tracks = tracks,
            totalDuration = null,
        )

    fun release(
        id: String = "rel-alb-1",
        albumId: String = "alb-1",
        trackGroups: List<BridgeTrackGroup> = listOf(trackGroup()),
        cover: BridgeImageRef? = imageRef(),
    ): BridgeRelease =
        BridgeRelease(
            id = id,
            albumId = albumId,
            name = BridgeReleaseName.Named("Release"),
            year = 2020,
            labels = listOf(BridgeLabelLine(names = listOf("Label A"), catalogNumbers = listOf("AB 100"))),
            facts =
                BridgePressingFacts(
                    area = BridgeReleaseArea.Country("US"),
                    media = listOf(BridgeMediaCount(BridgeMedium.VINYL, 1u)),
                    status = null,
                    packaging = null,
                    discogsDetails = emptyList(),
                ),
            // What core words from the facts above, written out because a
            // preview cannot call the bridge.
            pressingSummary =
                listOf(
                    BridgeFactTerm.Country("US"),
                    BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.medium.vinyl")),
                ),
            mediaTerms = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.medium.vinyl"))),
            pressingDetails = emptyList(),
            storageState = BridgeReleaseStorageState.LOCAL,
            pinned = false,
            storageActions = emptyList(),
            transferAction = null,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            files = emptyList(),
            sourceAudio = null,
            imageFiles = emptyList(),
            coverFiles = emptyList(),
            galleryItems = emptyList(),
            playability = BridgeReleasePlayability.PLAYABLE,
            records = emptyList(),
            totalDuration = null,
            fileCount = trackGroups.sumOf { it.tracks.size }.toLong(),
            totalSize = 300_000_000L,
            cover = cover,
        )

    fun albumDetail(
        album: BridgeAlbum = album(),
        releases: List<BridgeRelease> = listOf(release(albumId = album().id)),
    ): BridgeAlbumDetail = BridgeAlbumDetail(album = album, releases = releases)

    fun albumSearchResult(
        id: String = "alb-1",
        title: String = "Album Title",
    ): BridgeAlbumSearchResult =
        BridgeAlbumSearchResult(
            id = id,
            title = title,
            year = 2020,
            artistName = "Artist Name",
            cover = imageRef(),
        )

    fun trackSearchResult(
        id: String = "trk-1",
        releaseId: String = "rel-1",
        albumId: String = "alb-1",
    ): BridgeTrackSearchResult =
        BridgeTrackSearchResult(
            id = id,
            title = "Track Title",
            durationClock = trackDurationClock,
            releaseId = releaseId,
            albumId = albumId,
            albumTitle = "Album Title",
            artistName = "Artist Name",
            cover = imageRef(),
        )

    fun artistSummary(
        artistId: String = "artist-1",
        name: String = "Artist Name",
    ): BridgeArtistSummary =
        BridgeArtistSummary(
            artistId = artistId,
            name = name,
            albumCount = 4L,
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
            workCount = 3L,
            linkedReleaseCount = 5L,
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
            linkedReleaseCount = 2L,
            representativeReleaseId = "rel-1",
            representativeCover = null,
        )

    fun searchResults(
        albums: List<BridgeAlbumSearchResult> = listOf(albumSearchResult()),
        artists: List<BridgeArtistSummary> = listOf(artistSummary()),
        tracks: List<BridgeTrackSearchResult> = listOf(trackSearchResult()),
        composers: List<BridgeComposerSummary> = listOf(composerSummary()),
        works: List<BridgeWorkSummary> = listOf(workSummary()),
    ): BridgeSearchResults =
        BridgeSearchResults(
            albums = albums,
            artists = artists,
            tracks = tracks,
            composers = composers,
            works = works,
        )

    fun downloadOp(
        releaseId: String = "rel-1",
        state: BridgeDownloadState = BridgeDownloadState.Queued,
        title: String = "Album Title",
    ): BridgeDownloadOp =
        BridgeDownloadOp(
            releaseId = releaseId,
            release = BridgeQueuedRelease(title = title, fileCount = 10, totalSize = 300_000_000),
            createdAt = 0,
            state = state,
        )

    fun downloadSnapshot(
        queued: UInt = 1u,
        active: UInt = 1u,
        failed: UInt = 0u,
        paused: Boolean = false,
        downloads: List<BridgeDownloadOp> = listOf(downloadOp()),
    ): BridgeDownloadSnapshot =
        BridgeDownloadSnapshot(
            downloads = downloads,
            total = BridgeDownloadProgress(queued = queued, active = active, failed = failed),
            summaryParts = emptyList(),
            paused = paused,
        )

    fun library(
        id: String = "lib-1",
        name: String = "Library Name",
        isActive: Boolean = true,
    ): BridgeLibrary =
        BridgeLibrary(
            id = id,
            name = name,
            path = "/tmp/$id",
            storage = null,
            isActive = isActive,
        )

    fun downloadTransferProgress(
        bytesDone: ULong = 120_000_000uL,
        bytesTotal: ULong = 300_000_000uL,
        fraction: Double = 0.4,
    ): BridgeDownloadTransferProgress =
        BridgeDownloadTransferProgress(
            bytesDone = bytesDone,
            bytesTotal = bytesTotal,
            fraction = fraction,
        )

    /** The prompt after side B of a record ends, with the keys the bridge gives a side. */
    fun sidePausePrompt(): SidePausePrompt =
        SidePausePrompt(
            prompt =
                BridgeSidePausePrompt(
                    id = "prompt-1",
                    boundary = BridgePauseBoundary.SIDE,
                    sideLabel = "B",
                    countdown = null,
                ),
            titleKey = "core.playback.pause.side_ended.title",
            countdownKey = "core.playback.pause.side_ended.countdown",
            keepPausingKey = "core.playback.pause.side_ended.keep_pausing",
        )

    /** The prompt after disc 1 of a CD ends, with the keys the bridge gives a disc. */
    fun discPausePrompt(): SidePausePrompt =
        SidePausePrompt(
            prompt =
                BridgeSidePausePrompt(
                    id = "prompt-2",
                    boundary = BridgePauseBoundary.DISC,
                    sideLabel = "1",
                    countdown = null,
                ),
            titleKey = "core.playback.pause.disc_ended.title",
            countdownKey = "core.playback.pause.disc_ended.countdown",
            keepPausingKey = "core.playback.pause.disc_ended.keep_pausing",
        )

    fun galleryItem(
        id: String = "cover",
        label: String = "Cover",
        source: BridgeGallerySource = BridgeGallerySource.Cover(imageRef()),
    ): BridgeGalleryItem = BridgeGalleryItem(id = id, label = label, source = source)

    fun artistDetail(
        artist: BridgeArtistSummary = artistSummary(),
        albums: List<BridgeAlbum> = listOf(album()),
    ): BridgeArtistDetail = BridgeArtistDetail(artist = artist, albums = albums)

    fun member(
        id: String = "00000000-0000-4000-8000-000000000001",
        role: BridgeMemberRole = BridgeMemberRole.ADMIN,
        isSelf: Boolean = true,
    ): BridgeMember =
        BridgeMember(
            id = id,
            keys = BridgeMemberPublicKeys(signing = ByteArray(PUBLIC_KEY_BYTES), sealing = ByteArray(PUBLIC_KEY_BYTES)),
            role = role,
            devices =
                listOf(
                    BridgeDevice(id = 1uL, name = "Phone", state = BridgeDeviceState.ACTIVE, isSelf = isSelf),
                    BridgeDevice(id = 2uL, name = "Laptop", state = BridgeDeviceState.ACTIVE, isSelf = false),
                ),
            isSelf = isSelf,
            canRemove = !isSelf,
            canRemoveDevices = true,
        )

    fun person(member: BridgeMember = member()): BridgePerson =
        BridgePerson(
            id = member.id,
            profile =
                BridgePersonProfile(
                    id = member.id,
                    name = "Person Name",
                    privateCircle = "circle-1",
                    historySharing = BridgeSharing.PRIVATE,
                    avatarId = null,
                ),
            membership = member,
        )
}

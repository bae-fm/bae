package fm.bae.app.ui

import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LatestOne
import fm.bae.app.data.LatestSource
import fm.bae.app.playback.SidePausePrompt
import uniffi.bae_bridge.BridgeAbandonedAttachments
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferChange
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeKeyedDownloadTransfer
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryImageType
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberPublicKeys
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePauseBoundary
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgePendingRecords
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
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeUploadBar
import uniffi.bae_bridge.BridgeUploadTransfer
import uniffi.bae_bridge.BridgeUploadsInFlight
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
    private const val TRACK_DURATION_MS = 214_000uL
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
        number: Int = 1,
    ): BridgeTrack =
        BridgeTrack(
            id = id,
            title = title,
            durationMs = TRACK_DURATION_MS,
            durationClock = trackDurationClock,
            artistNames = "Artist Name",
            displayArtist = null,
            positionText = number.toString(),
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
            upload = null,
            transitioning = false,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            files = emptyList(),
            sourceAudio = null,
            imageFiles = emptyList(),
            coverFiles = emptyList(),
            galleryItems = emptyList(),
            playability = BridgeReleasePlayability.PLAYABLE,
            downloadStatus = null,
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

    /** A library mid-sync: files uploading and, when [needsAttention], a record
     *  that needs signing in. */
    fun syncActivity(needsAttention: Boolean): BridgeSyncActivity =
        BridgeSyncActivity(
            moving = true,
            needsAttention = needsAttention,
            failure = null,
            attention =
                BridgePendingRecords.Ready(
                    records =
                        if (needsAttention) {
                            listOf(
                                BridgePendingRecord(
                                    subject =
                                        BridgeMessage(key = "core.pending.subject.connection", args = emptyList()),
                                    reason = BridgePendingReason("core.pending.reason.storage", null, emptyList()),
                                    action = BridgePendingAction.SignIn(BridgeCloudProvider.DROPBOX),
                                    actionKey = "core.pending.action.sign_in",
                                ),
                            )
                        } else {
                            emptyList()
                        },
                ),
            abandoned = BridgeAbandonedAttachments.Ready(emptyList()),
            syncing = true,
            uploads =
                BridgeUploadsInFlight(
                    summaryParts =
                        listOf(
                            BridgeCountLabel(key = "core.queue.uploading", count = 2u),
                            BridgeCountLabel(key = "core.queue.queued", count = 10u),
                        ),
                ),
            downloads = null,
        )

    /** The upload queue's transfer while [syncActivity]'s files are sent. */
    fun uploadTransfer(): BridgeUploadTransfer =
        BridgeUploadTransfer(
            bar = BridgeUploadBar(bytesDone = 3_200_000uL, bytesTotal = 8_400_000uL),
            throughputBps = 0uL,
        )

    /** The files being sent together, as the sync activity's upload line reads them. */
    fun uploadTransferSource(): LatestSource<BridgeUploadTransfer> =
        LatestOne<BridgeUploadTransfer>().apply { set(uploadTransfer()) }.source

    fun downloadCounts(
        queued: UInt = 1u,
        active: UInt = 1u,
        failed: UInt = 0u,
        paused: Boolean = false,
    ): BridgeDownloadCounts =
        BridgeDownloadCounts(
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
            state = BridgeLibraryState.Openable(storage = null),
            isActive = isActive,
        )

    /** A download store holding [counts], with [downloadingReleaseId]'s download at [downloadTransferProgress]. */
    fun downloadStore(
        counts: BridgeDownloadCounts = downloadCounts(),
        downloadingReleaseId: String = "rel-1",
    ): DownloadStore =
        DownloadStore(counts).apply {
            val transfers =
                BridgeDownloadTransferChange.Reset(
                    listOf(BridgeKeyedDownloadTransfer(downloadingReleaseId, downloadTransferProgress())),
                )
            applyTransferChange(transfers)
        }

    fun downloadTransferProgress(
        bytesDone: ULong = 120_000_000uL,
        bytesTotal: ULong = 300_000_000uL,
        fraction: Double = 0.4,
    ): BridgeDownloadTransferProgress =
        BridgeDownloadTransferProgress(
            bytesDone = bytesDone,
            bytesTotal = bytesTotal,
            fraction = fraction,
            throughputBps = 4_800_000uL,
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
    ): BridgeArtistDetail = BridgeArtistDetail(artist = artist, creditNames = emptyList(), albums = albums)

    fun member(
        id: String = "00000000-0000-4000-8000-000000000001",
        role: BridgeMemberRole = BridgeMemberRole.ADMIN,
        isSelf: Boolean = true,
    ): BridgeMember =
        BridgeMember(
            id = id,
            keys = BridgeMemberPublicKeys(signing = ByteArray(PUBLIC_KEY_BYTES), sealing = ByteArray(PUBLIC_KEY_BYTES)),
            role = role,
            access = BridgeMemberAccess.S3AccessKey(accessKeyId = "AKIAPREVIEWKEY"),
            isSelf = isSelf,
            canRemove = !isSelf,
            activeDevices =
                listOf(
                    BridgeActiveDevice(id = 1uL, name = "Phone", isSelf = isSelf, canRemove = !isSelf),
                    BridgeActiveDevice(id = 2uL, name = "Laptop", isSelf = false, canRemove = true),
                ),
            formerDevices =
                listOf(
                    BridgeFormerDevice(id = 3uL, name = "Old Phone", state = BridgeFormerDeviceState.REPLACED),
                    BridgeFormerDevice(id = 4uL, name = "Tablet", state = BridgeFormerDeviceState.REMOVED),
                ),
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

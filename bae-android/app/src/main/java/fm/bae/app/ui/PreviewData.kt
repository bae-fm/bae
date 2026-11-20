package fm.bae.app.ui

import fm.bae.app.creditLineNamed
import fm.bae.app.data.DownloadStore
import fm.bae.app.playback.SidePausePrompt
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeCountdown
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadKind
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadQueueActivity
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGalleryLabel
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeHistorySharing
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryImageType
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgePressingFacts
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeQueuedRelease
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseArea
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeSideHeading
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeStorageFailure
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeTransferBatch
import uniffi.bae_bridge.BridgeUploadQueueActivity
import uniffi.bae_bridge.BridgeWorkSummary
import java.time.Instant

/**
 * `Bridge*` fixtures for the screenshot scenes and the tests, in the main source
 * set so they need no native library or `AppHandle`. Names are generic placeholders, never real
 * artists, albums or tracks.
 */
object PreviewData {
    // Sample track length (3:34) and the same value as a clock, built here
    // rather than through `bridgeClock` because a shot cannot
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
        artistNames: String = "Artist Name",
    ): BridgeAlbum =
        BridgeAlbum(
            id = id,
            title = title,
            year = year,
            isCompilation = false,
            artistLine = creditLineNamed(artistNames),
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
            artistLine = creditLineNamed("Artist Name"),
            displayArtist = null,
            positionText = number.toString(),
            works = emptyList(),
            myStars = null,
        )

    fun trackGroup(
        tracks: List<BridgeTrack> = listOf(track("trk-1", "Track Title", 1), track("trk-2", "Another Track", 2)),
    ): BridgeTrackGroup =
        BridgeTrackGroup(
            side = BridgeTrackSide.Flat,
            tracks = tracks,
            totalDuration = null,
            showsTime = false,
            // When a group has more than eight tracks, core starts a second column after half of them, rounded up.
            secondColumnStart = if (tracks.size > 8) ((tracks.size + 1) / 2).toUInt() else null,
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
            distinctYear = null,
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
            // shot cannot call the bridge.
            pressingSummary =
                listOf(
                    BridgeFactTerm.Country("US"),
                    BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.medium.vinyl")),
                ),
            mediaTerms = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.medium.vinyl"))),
            pressingDetails = emptyList(),
            storageState = BridgeReleaseStorageState.OnOneDevice,
            storageActions = emptyList(),
            upload = null,
            transitioning = false,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            credits = emptyList(),
            hasCredits = false,
            performsWorks = false,
            sourceAudio = null,
            otherFiles = emptyList(),
            hasOtherFiles = false,
            imageFiles = emptyList(),
            galleryItems = emptyList(),
            galleryItemCount = 0u,
            playability = BridgeReleasePlayability.Playable,
            moveToThisDevice = null,
            downloadStatus = null,
            records = emptyList(),
            hasRecords = false,
            totalDuration = null,
            fileCount = trackGroups.sumOf { it.tracks.size }.toLong(),
            totalSize = 300_000_000UL,
            cover = cover,
        )

    fun albumDetail(
        album: BridgeAlbum = album(),
        releases: List<BridgeRelease> = listOf(release(albumId = album().id)),
    ): BridgeAlbumDetail =
        BridgeAlbumDetail(
            album = album,
            releases = releases,
            shownRelease = releases.first(),
            shownReleaseIndex = 0u,
            shownReleaseCanBecomePrimary = false,
            showsReleaseChoices = releases.size > 1,
            rating = BridgeAlbumRating(mine = null, raters = emptyList()),
        )

    fun workSummary(
        workId: String = "work-1",
        title: String = "Work Title",
    ): BridgeWorkSummary =
        BridgeWorkSummary(
            workId = workId,
            title = title,
            composerNames = "Composer Name",
            linkedReleaseCount = 2L,
            representativeCover = null,
        )

    fun downloadOp(
        releaseId: String = "rel-1",
        state: BridgeDownloadState = BridgeDownloadState.Queued,
        title: String? = "Album Title",
        fileCount: ULong = 10UL,
        totalSize: ULong = 300_000_000UL,
    ): BridgeDownloadOp =
        BridgeDownloadOp(
            releaseId = releaseId,
            kind = BridgeDownloadKind.PIN,
            fileCount = fileCount,
            totalSize = totalSize,
            release = title?.let { BridgeQueuedRelease(title = it) },
            createdAt = 0,
            state = state,
        )

    /** A library mid-sync: releases uploading and pinning and, when [needsAttention], sync failing until the person
     *  signs in again. */
    fun syncActivity(needsAttention: Boolean): BridgeSyncActivity =
        BridgeSyncActivity(
            moving = true,
            held = false,
            failure =
                if (needsAttention) {
                    BridgeException.Diagnostic(
                        BridgeErrorCategory.Storage(BridgeStorageFailure.Authentication),
                        "Dropbox answered 401: expired_access_token",
                    )
                } else {
                    null
                },
            syncing = true,
            uploads =
                BridgeUploadQueueActivity(
                    batch = BridgeTransferBatch(done = 1u, total = 2u),
                    wait = null,
                ),
            pins =
                BridgeDownloadQueueActivity(
                    batch = BridgeTransferBatch(done = 1u, total = 3u),
                    wait = null,
                    paused = false,
                ),
            movingToThisDevice = null,
        )

    fun downloadCounts(
        queued: UInt = 1u,
        active: UInt = 1u,
        failed: UInt = 0u,
        wait: BridgeQueueWait? = null,
    ): BridgeDownloadCounts =
        BridgeDownloadCounts(
            total = BridgeDownloadProgress(queued = queued, active = active, failed = failed),
            hasEntries = queued + active + failed > 0u,
            hasFailures = failed > 0u,
            wait = wait,
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
            state = BridgeLibraryState.Openable(BridgeStorageLocation.S3("music", "us-east-1", null, null)),
            isActive = isActive,
        )

    /** A download store holding [counts], with [downloadingReleaseId]'s download at [transfer]. */
    fun downloadStore(
        counts: BridgeDownloadCounts = downloadCounts(),
        downloadingReleaseId: String = "rel-1",
        transfer: BridgeDownloadTransferProgress = downloadTransferProgress(),
    ): DownloadStore =
        DownloadStore(
            counts,
            initialUnmeteredOnly = false,
            releaseProgress = LatestStream.downloads(mapOf(downloadingReleaseId to transfer)),
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
            throughputBps = 4_800_000uL,
        )

    /** The prompt after side A of a record ends, with the keys the bridge gives a side, counting down to [until]
     *  when it is set. */
    fun sidePausePrompt(until: Instant? = null): SidePausePrompt =
        SidePausePrompt(
            prompt =
                BridgeSidePausePrompt(
                    id = "prompt-1",
                    ended = BridgeSideHeading.Sided(sideLetter = "A"),
                    countdown =
                        until?.let { BridgeCountdown(key = "core.playback.pause.side_ended.countdown", until = it) },
                ),
            titleKey = "core.playback.pause.side_ended.title",
            keepPausingKey = "core.playback.pause.side_ended.keep_pausing",
        )

    /** The prompt after disc 2 of a CD box ends, with the keys the bridge gives a disc, counting down to [until]
     *  when it is set. */
    fun discPausePrompt(until: Instant? = null): SidePausePrompt =
        SidePausePrompt(
            prompt =
                BridgeSidePausePrompt(
                    id = "prompt-2",
                    ended = BridgeSideHeading.Disc(disc = 2u),
                    countdown =
                        until?.let { BridgeCountdown(key = "core.playback.pause.disc_ended.countdown", until = it) },
                ),
            titleKey = "core.playback.pause.disc_ended.title",
            keepPausingKey = "core.playback.pause.disc_ended.keep_pausing",
        )

    fun galleryItem(
        id: String = "cover",
        label: BridgeGalleryLabel = BridgeGalleryLabel.Cover,
        source: BridgeGallerySource = BridgeGallerySource.Cover(imageRef()),
    ): BridgeGalleryItem = BridgeGalleryItem(id = id, label = label, source = source)

    fun member(
        id: String = "00000000-0000-4000-8000-000000000001",
        role: BridgeMemberRole = BridgeMemberRole.ADMIN,
        isSelf: Boolean = true,
    ): BridgeMember =
        BridgeMember(
            id = id,
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
            activeDeviceCount = 2u,
            formerDeviceCount = 2u,
        )

    fun person(member: BridgeMember = member()): BridgePerson =
        BridgePerson(
            profile =
                BridgePersonProfile(
                    id = member.id,
                    name = "Person Name",
                    initials = "PN",
                    historySharing = BridgeHistorySharing.PRIVATE,
                    avatarVersion = null,
                ),
            membership = member,
        )
}

package fm.bae.app.ui

import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeArtistDetail
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadProgress
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeGalleryItem
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeInviteCodeInfo
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryImageType
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseStorageState
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeSyncConfig
import uniffi.bae_bridge.BridgeSyncProvider
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSearchResult
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeWorkSummary

/**
 * Plain `Bridge*` fixtures for `@Preview`s. Mirrors the shapes the test-only
 * `BridgeFixtures` builds, but lives in the main source set so previews can draw
 * on it without the native lib or an `AppHandle`. Every name is a generic
 * placeholder — never a real artist, album, or track — so previews carry no real
 * catalogue data. Previews stub the image loaders (`suspend (id) -> null`), so
 * these carry no cover bytes.
 */
object PreviewData {
    // Placeholder 64-hex-char device public keys (kept out of expression bodies so
    // the length literal reads as a named property, not an inline magic number).
    private val placeholderPubkey = "0".repeat(64)
    private val placeholderPubkeyAlt = "1".repeat(64)

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
            durationMs = 214_000L,
            artistNames = "Artist Name",
            displayArtist = null,
            positionText = trackNumber?.toString() ?: "",
        )

    fun trackGroup(
        tracks: List<BridgeTrack> = listOf(track("trk-1", "Track Title", 1), track("trk-2", "Another Track", 2)),
    ): BridgeTrackGroup = BridgeTrackGroup(side = BridgeTrackSide.Flat, tracks = tracks)

    fun release(
        id: String = "rel-alb-1",
        albumId: String = "alb-1",
        trackGroups: List<BridgeTrackGroup> = listOf(trackGroup()),
        cover: BridgeImageRef? = imageRef(),
    ): BridgeRelease =
        BridgeRelease(
            id = id,
            albumId = albumId,
            displayName = "Release",
            year = 2020,
            format = "LP",
            label = "Label Name",
            catalogNumber = "CAT-001",
            country = "US",
            storageState = BridgeReleaseStorageState.LOCAL,
            pinned = false,
            storageActions = emptyList(),
            transferAction = null,
            tracks = trackGroups.flatMap { it.tracks },
            trackGroups = trackGroups,
            files = emptyList(),
            imageFiles = emptyList(),
            galleryItems = emptyList(),
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
        albumId: String = "alb-1",
    ): BridgeTrackSearchResult =
        BridgeTrackSearchResult(
            id = id,
            title = "Track Title",
            durationMs = 214_000L,
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
            title = title,
            fileCount = 10,
            totalSize = 300_000_000,
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
            cloudProvider = null,
            isActive = isActive,
            error = null,
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

    fun sidePausePrompt(): BridgeSidePausePrompt =
        BridgeSidePausePrompt(
            id = "prompt-1",
            titleKey = "core.playback.side_pause.title",
            sideLetter = "B",
            messageKey = "core.playback.side_pause.message",
        )

    fun inviteCodeInfo(
        cloudProvider: BridgeCloudProvider = BridgeCloudProvider.S3,
        needsOauth: Boolean = false,
    ): BridgeInviteCodeInfo =
        BridgeInviteCodeInfo(
            libraryId = "lib-1",
            libraryName = "Library Name",
            ownerPubkey = placeholderPubkey,
            ownerFingerprint = "0a1b2c3d",
            cloudProvider = cloudProvider,
            needsOauth = needsOauth,
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
        pubkey: String = placeholderPubkey,
        role: BridgeMemberRole = BridgeMemberRole.OWNER,
        isSelf: Boolean = true,
        fingerprint: String = "0a1b2c3d",
        canRemove: Boolean = false,
    ): BridgeMember =
        BridgeMember(
            pubkey = pubkey,
            role = role,
            isSelf = isSelf,
            fingerprint = fingerprint,
            canRemove = canRemove,
        )

    fun membership(
        members: List<BridgeMember> =
            listOf(
                member(),
                member(
                    pubkey = placeholderPubkeyAlt,
                    role = BridgeMemberRole.MEMBER,
                    isSelf = false,
                    fingerprint = "1a2b3c4d",
                    canRemove = true,
                ),
            ),
        selfIsOwner: Boolean = true,
    ): BridgeMembership = BridgeMembership(members = members, selfIsOwner = selfIsOwner)

    fun syncConfig(
        provider: BridgeSyncProvider =
            BridgeSyncProvider.S3(bucket = "bucket-name", region = "us-east-1", endpoint = null),
        cloudAccountDisplay: String? = "s3://bucket-name",
    ): BridgeSyncConfig = BridgeSyncConfig(provider = provider, cloudAccountDisplay = cloudAccountDisplay)
}

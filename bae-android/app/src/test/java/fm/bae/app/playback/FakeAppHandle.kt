package fm.bae.app.playback

import fm.bae.app.BridgeFixtures
import kotlinx.coroutines.CompletableDeferred
import uniffi.bae_bridge.AddDeviceStatusSubscription
import uniffi.bae_bridge.AdminChangesSubscription
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailSubscription
import uniffi.bae_bridge.AlbumSelectionSubscription
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.ArtistBrowseSubscription
import uniffi.bae_bridge.ArtistDetailSubscription
import uniffi.bae_bridge.ArtistSearchSubscription
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddDeviceRequest
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDeviceCutOff
import uniffi.bae_bridge.BridgeDeviceRemoval
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeHistorySharing
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeImportDestination
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteLifetime
import uniffi.bae_bridge.BridgeInviteScreenHold
import uniffi.bae_bridge.BridgeJoinRequest
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeLookupCatalog
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgeOutboxFeeds
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePreviewTarget
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeRendererService
import uniffi.bae_bridge.BridgeRendererServiceType
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeReportedRenderer
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.BridgeSaveFilenamePiece
import uniffi.bae_bridge.BridgeSaveFolderPiece
import uniffi.bae_bridge.BridgeSavePreset
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSetupCancellation
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.BridgeSidePauseCountdown
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeStorageFilter
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeStorageSort
import uniffi.bae_bridge.BridgeStoreKeyState
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeUploadReleasesOutcome
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.CastReceiversSubscription
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailRowsSubscription
import uniffi.bae_bridge.ComposerDetailSubscription
import uniffi.bae_bridge.ConfigSubscription
import uniffi.bae_bridge.DeviceStorageSubscription
import uniffi.bae_bridge.DownloadBrowseSubscription
import uniffi.bae_bridge.DownloadCountsSubscription
import uniffi.bae_bridge.InvitesSubscription
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.LibraryNameSubscription
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.MembersSubscription
import uniffi.bae_bridge.PlaybackTimelineSubscription
import uniffi.bae_bridge.PlaybackValuesSubscription
import uniffi.bae_bridge.PlaylistDetailSubscription
import uniffi.bae_bridge.PlaylistsSubscription
import uniffi.bae_bridge.ProfilesSubscription
import uniffi.bae_bridge.QueueAdditionsSubscription
import uniffi.bae_bridge.QueueSubscription
import uniffi.bae_bridge.QueueUpcomingSubscription
import uniffi.bae_bridge.ReleaseDetailSubscription
import uniffi.bae_bridge.RequestedProfileSubscription
import uniffi.bae_bridge.StorageBrowseSubscription
import uniffi.bae_bridge.SyncActivitySubscription
import uniffi.bae_bridge.SyncStatusSubscription
import uniffi.bae_bridge.UnmeteredOnlySubscription
import uniffi.bae_bridge.UploadsPausedSubscription
import uniffi.bae_bridge.WorkDetailSubscription

/** A bridge call a test fake doesn't fake: fail naming it. */
internal fun unfaked(method: String): Nothing = throw AssertionError("the test reached $method, which its fake doesn't fake")

/**
 * The bridge handle the Android tests run against. It implements every call of
 * [AppHandleInterface], so a call the bridge gains is a compile error here, not
 * a test that reaches the native library with no handle. A call no test fakes
 * fails with [unfaked].
 */
internal open class FakeAppHandle(
    private val albumPages: (offset: ULong, limit: ULong) -> List<BridgeAlbum> = { _, _ -> emptyList() },
    private val composerPages: (offset: ULong, limit: ULong) -> List<BridgeComposerSummary> = { _, _ -> emptyList() },
    private val albumDetails: Map<String, BridgeAlbumDetail> = emptyMap(),
    private val composerDetails: Map<String, BridgeComposerDetail> = emptyMap(),
    private val composerDetailRows: Map<String, List<BridgeComposerDetailRow>> = emptyMap(),
    private val workDetails: Map<String, BridgeWorkDetail> = emptyMap(),
    private val releaseDetails: Map<String, BridgeRelease> = emptyMap(),
    private val searchResults: (query: String) -> BridgeSearchResults = { BridgeFixtures.searchResults() },
    private val initialAlbumPageError: uniffi.bae_bridge.BridgeException? = null,
    private val initialSearchError: (query: String) -> uniffi.bae_bridge.BridgeException? = { null },
    var deliverAlbumPagesImmediately: Boolean = true,
    var deliverAlbumBaselineImmediately: Boolean = true,
    var deliverSearchResultsImmediately: Boolean = true,
) : EditionFakeAppHandle() {
    var pauseCount = 0
    var resumeCount = 0

    /** Offset/limit of each window the album browse was asked for — lets
     *  browse-paging tests assert the requested window reached the bridge
     *  unaltered. */
    val albumPageWindows = mutableListOf<Pair<ULong, ULong>>()
    val playReleaseCalls = mutableListOf<Triple<String, String?, Boolean>>()
    val playTracksCalls = mutableListOf<Pair<List<String>, UInt>>()
    val albumBrowseSubscriptions = mutableListOf<FakeAlbumBrowseSubscription>()

    /** The first album browse the handle opened, once it has. */
    val firstAlbumBrowse = CompletableDeferred<FakeAlbumBrowseSubscription>()
    val composerBrowseSubscriptions = mutableListOf<FakeComposerBrowseSubscription>()
    val albumDetailSubscriptions = mutableListOf<FakeAlbumDetailSubscription>()
    val searchSubscriptions = mutableListOf<FakeLibrarySearchSubscription>()

    override fun pause() {
        pauseCount++
    }

    override fun resume() {
        resumeCount++
    }

    override suspend fun savePlaybackState() {}

    override fun subscribeConfig(): ConfigSubscription = FakeConfigSubscription()

    /** The sync status the handle answers, which a test delivers to. */
    val syncStatus = FakeSyncStatusSubscription()

    override fun subscribeSyncStatus(): SyncStatusSubscription = syncStatus

    /** The sync activity the handle answers, which a test delivers to. */
    val syncActivity = FakeSyncActivitySubscription()

    override fun subscribeSyncActivity(): SyncActivitySubscription = syncActivity

    override fun subscribeDownloadCounts(): DownloadCountsSubscription = FakeDownloadCountsSubscription()

    override fun subscribeDeviceStorage(): DeviceStorageSubscription = unfaked("subscribeDeviceStorage")

    override fun subscribeDownloadBrowse(): DownloadBrowseSubscription = unfaked("subscribeDownloadBrowse")

    override fun feedDownloadTransfers(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedMoveToThisDeviceTransfers(): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeUploadsPaused(): UploadsPausedSubscription = FakeUploadsPausedSubscription()

    override fun subscribeUnmeteredOnly(): UnmeteredOnlySubscription = FakeUnmeteredOnlySubscription()

    override fun feedOutboxTransfers(): BridgeOutboxFeeds = BridgeOutboxFeeds.Streams(FakeKeyedValueStream(), FakeKeyedValueStream())

    override fun feedUploadFileShares(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedPinFileShares(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedActivityBars(): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeCastReceivers(): CastReceiversSubscription = FakeCastReceiversSubscription()

    override fun subscribeQueue(): QueueSubscription = FakeQueueSubscription()

    /** The playback values the handle answers, which a test delivers to. */
    val playbackValues = FakePlaybackValuesSubscription()

    override fun subscribePlaybackValues(): PlaybackValuesSubscription = playbackValues

    override fun subscribeQueueAdditions(): QueueAdditionsSubscription = FakeQueueAdditionsSubscription()

    /** The playback timeline the handle answers, which a test delivers to. */
    val playbackTimeline = FakePlaybackTimelineSubscription()

    override fun subscribePlaybackTimeline(): PlaybackTimelineSubscription = playbackTimeline

    override fun feedPlaybackPositions(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedTrackBuffered(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedTrackStorageWaits(): KeyedValueStream = FakeKeyedValueStream()

    override fun triggerSync() {}

    override suspend fun fetchLibraryImageBytes(image: BridgeImageRef): ByteArray? = null

    /** Deliver [value] on a search, as the answer to its newest request unless [revision] names
     *  an earlier one. */
    fun emitSearchResults(
        subscription: Int,
        value: BridgeSearchResults,
        revision: ULong? = null,
    ) {
        searchSubscriptions[subscription].emit(Result.success(value), revision)
    }

    override fun subscribeAlbumBrowse(
        sortCriteria: List<BridgeSortCriterion>,
        groupByArtist: Boolean,
    ): AlbumBrowseSubscription =
        FakeAlbumBrowseSubscription(
            albumPages,
            albumPageWindows,
            deliverAlbumPagesImmediately,
            deliverAlbumBaselineImmediately,
            initialAlbumPageError,
        ).also {
            albumBrowseSubscriptions += it
            firstAlbumBrowse.complete(it)
        }

    override fun subscribeComposerBrowse(sortCriteria: List<BridgeComposerSortCriterion>): ComposerBrowseSubscription =
        FakeComposerBrowseSubscription(composerPages).also(composerBrowseSubscriptions::add)

    override fun subscribeAlbumDetail(): AlbumDetailSubscription =
        FakeAlbumDetailSubscription(FakeDetailRead { albumDetails[it] }).also(albumDetailSubscriptions::add)

    override fun subscribeComposerDetail(): ComposerDetailSubscription =
        FakeComposerDetailSubscription(FakeDetailRead { composerDetails[it] })

    override fun subscribeComposerDetailRows(artistId: String): ComposerDetailRowsSubscription =
        FakeComposerDetailRowsSubscription(composerDetailRows[artistId].orEmpty())

    override fun subscribeWorkDetail(): WorkDetailSubscription = FakeWorkDetailSubscription(FakeDetailRead { workDetails[it] })

    override fun subscribeReleaseDetail(): ReleaseDetailSubscription = FakeReleaseDetailSubscription(FakeDetailRead { releaseDetails[it] })

    override fun subscribeLibrarySearch(): LibrarySearchSubscription =
        FakeLibrarySearchSubscription { query ->
            if (!deliverSearchResultsImmediately) {
                null
            } else {
                initialSearchError(query)?.let { Result.failure(it) } ?: Result.success(searchResults(query))
            }
        }.also(searchSubscriptions::add)

    fun failSearchResults(
        subscription: Int,
        error: uniffi.bae_bridge.BridgeException,
    ) {
        searchSubscriptions[subscription].emit(Result.failure(error))
    }

    override suspend fun playRelease(
        releaseId: String,
        startTrackId: String?,
        shuffle: Boolean,
    ): BridgePlayOutcome {
        playReleaseCalls.add(Triple(releaseId, startTrackId, shuffle))
        return BridgePlayOutcome.Playing
    }

    override suspend fun playTracks(
        trackIds: List<String>,
        startIndex: UInt,
    ): BridgePlayOutcome {
        playTracksCalls.add(trackIds to startIndex)
        return BridgePlayOutcome.Playing
    }

    /** Closing the fake releases nothing. */
    override fun close() {}

    // Every other call the bridge offers: a test that reaches one fakes it here first.

    override fun closeLibrary(closing: BridgeLibraryClosing?) {
        unfaked("closeLibrary")
    }

    override suspend fun storageLocation(): BridgeStorageLocation? = unfaked("storageLocation")

    override fun subscribeAlbumSelection(): AlbumSelectionSubscription = unfaked("subscribeAlbumSelection")

    override suspend fun addNext(trackIds: List<String>) {
        unfaked("addNext")
    }

    override suspend fun addReleaseNext(releaseId: String) {
        unfaked("addReleaseNext")
    }

    override suspend fun addReleaseToQueue(releaseId: String) {
        unfaked("addReleaseToQueue")
    }

    override suspend fun addToQueue(trackIds: List<String>) {
        unfaked("addToQueue")
    }

    override suspend fun cancelDownload(releaseId: String) {
        unfaked("cancelDownload")
    }

    override fun cancelSidePauseCountdown() {
        unfaked("cancelSidePauseCountdown")
    }

    override suspend fun clearCache(): Unit = unfaked("clearCache")

    override suspend fun clearPlayingFrom() {
        unfaked("clearPlayingFrom")
    }

    override suspend fun clearUpNext() {
        unfaked("clearUpNext")
    }

    override suspend fun deleteDataFromBeforeReset(): Unit = unfaked("deleteDataFromBeforeReset")

    override suspend fun deleteRelease(releaseId: String) {
        unfaked("deleteRelease")
    }

    override suspend fun deleteAlbum(albumId: String) {
        unfaked("deleteAlbum")
    }

    override suspend fun generateRestoreCode(): BridgeRestoreCodeShare = unfaked("generateRestoreCode")

    override suspend fun addDevice(): BridgeAddDevice = unfaked("addDevice")

    override suspend fun getAlbumIndex(
        sortCriteria: List<BridgeSortCriterion>,
        albumId: String,
        groupByArtist: Boolean,
    ): ULong? = unfaked("getAlbumIndex")

    override suspend fun albumsInBrowseOrder(
        sortCriteria: List<BridgeSortCriterion>,
        albumIds: List<String>,
        groupByArtist: Boolean,
    ): List<BridgeAlbum> = unfaked("albumsInBrowseOrder")

    override suspend fun getDownloadCounts(): BridgeDownloadCounts = unfaked("getDownloadCounts")

    override suspend fun insertInQueue(
        trackIds: List<String>,
        index: UInt,
    ) {
        unfaked("insertInQueue")
    }

    override fun nextTrack() {
        unfaked("nextTrack")
    }

    override suspend fun playLibraryShuffled(): BridgePlayOutcome = unfaked("playLibraryShuffled")

    override suspend fun playReleases(releaseIds: List<String>): BridgePlayOutcome = unfaked("playReleases")

    override fun previewPlay(target: BridgePreviewTarget) {
        unfaked("previewPlay")
    }

    override fun previewSeekByRatio(ratio: Double) {
        unfaked("previewSeekByRatio")
    }

    override fun previewSetPaused(paused: Boolean) {
        unfaked("previewSetPaused")
    }

    override fun previewStop() {
        unfaked("previewStop")
    }

    override fun previewTogglePause() {
        unfaked("previewTogglePause")
    }

    override fun previousTrack() {
        unfaked("previousTrack")
    }

    override suspend fun queuePinReleases(releaseIds: List<String>) {
        unfaked("queuePinReleases")
    }

    override suspend fun removeEntry(entryId: String) {
        unfaked("removeEntry")
    }

    override suspend fun reorderEntry(
        entryId: String,
        beforeEntryId: String?,
    ): ULong? = unfaked("reorderEntry")

    override suspend fun resolveToTrackIds(ids: List<String>): List<String> = unfaked("resolveToTrackIds")

    override suspend fun retryDownload(releaseId: String) {
        unfaked("retryDownload")
    }

    override suspend fun retryDownloads() {
        unfaked("retryDownloads")
    }

    override suspend fun saveSyncConfig(
        configData: BridgeSaveSyncConfig,
        deviceName: String,
        setup: BridgeLibrarySetup?,
    ): BridgeStorageSetup = unfaked("saveSyncConfig")

    override fun subscribeArtistSearch(): ArtistSearchSubscription = unfaked("subscribeArtistSearch")

    override fun seekByRatio(ratio: Double) {
        unfaked("seekByRatio")
    }

    override suspend fun setDownloadsPaused(paused: Boolean) {
        unfaked("setDownloadsPaused")
    }

    override fun setMuted(muted: Boolean) {
        unfaked("setMuted")
    }

    override suspend fun setSaveCoverFile(on: Boolean) {
        unfaked("setSaveCoverFile")
    }

    override suspend fun setSaveEmbedCover(on: Boolean) {
        unfaked("setSaveEmbedCover")
    }

    override suspend fun setSaveLoudnessTags(on: Boolean) {
        unfaked("setSaveLoudnessTags")
    }

    override suspend fun setFolderPieces(pieces: List<BridgeSaveFolderPiece>) {
        unfaked("setFolderPieces")
    }

    override suspend fun setFilenamePieces(pieces: List<BridgeSaveFilenamePiece>) {
        unfaked("setFilenamePieces")
    }

    override suspend fun setPrimaryRelease(
        albumId: String,
        releaseId: String,
    ) {
        unfaked("setPrimaryRelease")
    }

    override suspend fun setRepeatMode(mode: BridgeRepeatMode) {
        unfaked("setRepeatMode")
    }

    override suspend fun setShuffle(on: Boolean) {
        unfaked("setShuffle")
    }

    override fun setVolume(volume: Float) {
        unfaked("setVolume")
    }

    override fun setVolumeLive(volume: Float) {
        unfaked("setVolumeLive")
    }

    override suspend fun skipToEntry(entryId: String) {
        unfaked("skipToEntry")
    }

    override fun stop() {
        unfaked("stop")
    }

    override suspend fun unpinReleases(releaseIds: List<String>) {
        unfaked("unpinReleases")
    }

    override suspend fun uploadReleases(releaseIds: List<String>): BridgeUploadReleasesOutcome = unfaked("uploadReleases")

    override suspend fun fetchReleaseImageBytes(
        releaseId: String,
        source: BridgeGallerySource,
    ): ByteArray = unfaked("fetchReleaseImageBytes")

    override fun subscribeArtistBrowse(sortCriteria: List<BridgeArtistSortCriterion>): ArtistBrowseSubscription =
        unfaked("subscribeArtistBrowse")

    override suspend fun lockActiveLibrary() {
        unfaked("lockActiveLibrary")
    }

    override suspend fun renameLibrary(name: String) {
        unfaked("renameLibrary")
    }

    override suspend fun setIdentifyAutomatically(enabled: Boolean) {
        unfaked("setIdentifyAutomatically")
    }

    override suspend fun setImportDestination(destination: BridgeImportDestination) {
        unfaked("setImportDestination")
    }

    override suspend fun setImportWhenIdentified(enabled: Boolean) {
        unfaked("setImportWhenIdentified")
    }

    override suspend fun setLibraryFullWidth(enabled: Boolean) {
        unfaked("setLibraryFullWidth")
    }

    override suspend fun setCacheLimit(choice: BridgeCacheLimitChoice): Unit = unfaked("setCacheLimit")

    override suspend fun setAskBeforeUnpinning(enabled: Boolean) {
        unfaked("setAskBeforeUnpinning")
    }

    override suspend fun setMaxConcurrentDownloads(n: UInt) {
        unfaked("setMaxConcurrentDownloads")
    }

    override suspend fun setMaxConcurrentUploads(n: UInt) {
        unfaked("setMaxConcurrentUploads")
    }

    override suspend fun setMetadataSourceEnabled(
        source: BridgeLookupCatalog,
        enabled: Boolean,
    ) {
        unfaked("setMetadataSourceEnabled")
    }

    override suspend fun setPauseBetweenSides(enabled: Boolean) {
        unfaked("setPauseBetweenSides")
    }

    override suspend fun setSavePresets(presets: List<BridgeSavePreset>) {
        unfaked("setSavePresets")
    }

    override suspend fun setShowRemainingTime(enabled: Boolean) {
        unfaked("setShowRemainingTime")
    }

    override suspend fun setSidePauseCountdown(countdown: BridgeSidePauseCountdown) {
        unfaked("setSidePauseCountdown")
    }

    override fun storeKeyState(): BridgeStoreKeyState = unfaked("storeKeyState")

    override suspend fun unlockStoreKey() {
        unfaked("unlockStoreKey")
    }

    override suspend fun castTo(deviceId: String) {
        unfaked("castTo")
    }

    override fun getRendererServiceTypes(): List<BridgeRendererService> = unfaked("getRendererServiceTypes")

    override fun rendererFound(service: BridgeReportedRenderer) {
        unfaked("rendererFound")
    }

    override fun rendererLost(
        serviceType: BridgeRendererServiceType,
        instanceName: String,
    ) {
        unfaked("rendererLost")
    }

    override suspend fun setCastEnabled(enabled: Boolean) {
        unfaked("setCastEnabled")
    }

    override suspend fun setCastTranscodeFormat(format: BridgeCastTranscodeFormat) {
        unfaked("setCastTranscodeFormat")
    }

    override fun startCastDiscovery(asker: BridgeBrowsingFor) {
        unfaked("startCastDiscovery")
    }

    override fun stopCastDiscovery(asker: BridgeBrowsingFor) {
        unfaked("stopCastDiscovery")
    }

    override fun stopCasting() {
        unfaked("stopCasting")
    }

    override fun subscribeArtistDetail(): ArtistDetailSubscription = unfaked("subscribeArtistDetail")

    override suspend fun approveAddDevice(request: BridgeAddDeviceRequest) {
        unfaked("approveAddDevice")
    }

    override suspend fun declineAddDevice(request: BridgeAddDeviceRequest) {
        unfaked("declineAddDevice")
    }

    override suspend fun endAddDevice(pairing: String) {
        unfaked("endAddDevice")
    }

    override suspend fun approveJoinRequest(request: BridgeJoinRequest) {
        unfaked("approveJoinRequest")
    }

    override suspend fun cancelInvite(invite: String) {
        unfaked("cancelInvite")
    }

    override suspend fun declineJoinRequest(request: BridgeJoinRequest) {
        unfaked("declineJoinRequest")
    }

    override suspend fun getMembers(): BridgeMembership = unfaked("getMembers")

    override suspend fun invitePerson(
        role: BridgeMemberRole,
        access: BridgeInviteAccess,
        lifetime: BridgeInviteLifetime,
    ): String = unfaked("invitePerson")

    override fun holdInviteScreenSync(): BridgeInviteScreenHold = unfaked("holdInviteScreenSync")

    override suspend fun people(): List<BridgePerson> = unfaked("people")

    override suspend fun ownPerson(): BridgePerson? = unfaked("ownPerson")

    override suspend fun removeDevice(device: ULong): BridgeDeviceRemoval = unfaked("removeDevice")

    override suspend fun confirmDeviceCutOff(device: ULong): BridgeDeviceCutOff = unfaked("confirmDeviceCutOff")

    override suspend fun removeMember(member: String) {
        unfaked("removeMember")
    }

    override suspend fun setMemberRole(
        member: String,
        role: BridgeMemberRole,
    ) {
        unfaked("setMemberRole")
    }

    /** This device's open invites the handle answers, which a test delivers to. */
    val invites = FakeInvitesSubscription()

    override fun subscribeInvites(): InvitesSubscription = invites

    /** The admin changes the handle answers, which a test delivers to. */
    val adminChanges = FakeAdminChangesSubscription()

    override fun subscribeAdminChanges(): AdminChangesSubscription = adminChanges

    override suspend fun dismissRefusedInvite(operation: Long) {
        unfaked("dismissRefusedInvite")
    }

    override suspend fun acknowledgeAdminChange(operation: Long) {
        unfaked("acknowledgeAdminChange")
    }

    override fun subscribeMembers(): MembersSubscription = unfaked("subscribeMembers")

    override fun followAddDevice(pairing: String): AddDeviceStatusSubscription = unfaked("followAddDevice")

    override suspend fun appendPlaylistEntry(
        playlist: String,
        track: String,
    ): String = unfaked("appendPlaylistEntry")

    override suspend fun avatar(person: String): ByteArray? = unfaked("avatar")

    override suspend fun createPlaylist(
        name: String,
        sharing: BridgeSharing,
        trackIds: List<String>,
    ): String = unfaked("createPlaylist")

    override suspend fun createProfile(name: String) {
        unfaked("createProfile")
    }

    override suspend fun deletePlaylist(id: String) {
        unfaked("deletePlaylist")
    }

    override suspend fun libraryName(): String = unfaked("libraryName")

    override suspend fun playPlaylist(
        playlistId: String,
        startPositionId: String?,
        shuffle: Boolean,
    ): BridgePlayOutcome = unfaked("playPlaylist")

    override suspend fun removePlaylistEntry(
        playlist: String,
        position: String,
    ) {
        unfaked("removePlaylistEntry")
    }

    override suspend fun renamePlaylist(
        id: String,
        name: String,
    ) {
        unfaked("renamePlaylist")
    }

    override suspend fun renameProfile(name: String) {
        unfaked("renameProfile")
    }

    override suspend fun reorderPlaylist(
        playlist: String,
        order: List<String>,
    ) {
        unfaked("reorderPlaylist")
    }

    override suspend fun setAvatar(bytes: ByteArray?) {
        unfaked("setAvatar")
    }

    override suspend fun setHistorySharing(sharing: BridgeHistorySharing) {
        unfaked("setHistorySharing")
    }

    override suspend fun setPlaylistSharing(
        id: String,
        sharing: BridgeSharing,
    ) {
        unfaked("setPlaylistSharing")
    }

    override suspend fun setRating(
        target: BridgeRatingTarget,
        stars: BridgeStars?,
    ) {
        unfaked("setRating")
    }

    override fun subscribeLibraryName(): LibraryNameSubscription = unfaked("subscribeLibraryName")

    override fun subscribePlaylistDetail(): PlaylistDetailSubscription = unfaked("subscribePlaylistDetail")

    override fun subscribePlaylists(): PlaylistsSubscription = unfaked("subscribePlaylists")

    override fun subscribeProfiles(): ProfilesSubscription = unfaked("subscribeProfiles")

    /** The profile this device's person asked for, which a test delivers to. */
    val requestedProfile = FakeRequestedProfileSubscription()

    override fun subscribeRequestedProfile(): RequestedProfileSubscription = requestedProfile

    override fun subscribeQueueUpcoming(): QueueUpcomingSubscription = unfaked("subscribeQueueUpcoming")

    override suspend fun removeLibrary(closing: BridgeLibraryClosing?): BridgeLibraryRemoval = unfaked("removeLibrary")

    override fun subscribeStorageBrowse(
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
    ): StorageBrowseSubscription = unfaked("subscribeStorageBrowse")

    override suspend fun cancelReleaseTransition(releaseId: String) {
        unfaked("cancelReleaseTransition")
    }

    override suspend fun getUploadsPaused(): Boolean = unfaked("getUploadsPaused")

    override suspend fun getUnmeteredOnly(): Boolean = unfaked("getUnmeteredOnly")

    override suspend fun reconnectSync() {
        unfaked("reconnectSync")
    }

    /** Every session tells its handle the app's language as it starts, so a fake that refused would fail every session's start. */
    override suspend fun setSortLanguage(tag: String) {}

    override suspend fun releaseMemory() {
        unfaked("releaseMemory")
    }

    override suspend fun retryNow(): BridgeSyncRetryOutcome = unfaked("retryNow")

    override suspend fun retryOutbox() {
        unfaked("retryOutbox")
    }

    override suspend fun retryPendingOperation(id: Long) {
        unfaked("retryPendingOperation")
    }

    override fun cancelStorageSetup(): BridgeSetupCancellation = unfaked("cancelStorageSetup")

    override suspend fun dismissUndoneStorageSetup(operation: Long) {
        unfaked("dismissUndoneStorageSetup")
    }

    override suspend fun cancelFailedReleaseMove(id: Long) {
        unfaked("cancelFailedReleaseMove")
    }

    override suspend fun setUnmeteredOnly(unmeteredOnly: Boolean) {
        unfaked("setUnmeteredOnly")
    }

    override suspend fun setSyncPaused(paused: Boolean) {
        unfaked("setSyncPaused")
    }
}

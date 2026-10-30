package fm.bae.app.playback

import fm.bae.app.BridgeFixtures
import uniffi.bae_bridge.AddedDeviceCallback
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailSubscription
import uniffi.bae_bridge.AlbumSelectionSubscription
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.ArtistBrowseSubscription
import uniffi.bae_bridge.ArtistDetailSubscription
import uniffi.bae_bridge.ArtistSearchSubscription
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddedDeviceWatch
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeCoverSelection
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteDelivery
import uniffi.bae_bridge.BridgeJoinRequest
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgePlaylist
import uniffi.bae_bridge.BridgePlaylistEntry
import uniffi.bae_bridge.BridgePreviewTarget
import uniffi.bae_bridge.BridgeProviderSignOut
import uniffi.bae_bridge.BridgeRating
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeRendererService
import uniffi.bae_bridge.BridgeRendererServiceType
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeReportedRenderer
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeS3Detection
import uniffi.bae_bridge.BridgeSavePreset
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeSearchResults
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
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeUnsentWork
import uniffi.bae_bridge.BridgeUploadReleasesOutcome
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.CastDevicesCallback
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailSubscription
import uniffi.bae_bridge.ConfigCallback
import uniffi.bae_bridge.DownloadBrowseSubscription
import uniffi.bae_bridge.DownloadCountsCallback
import uniffi.bae_bridge.InvitesCallback
import uniffi.bae_bridge.KeyActivityCallback
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.LibraryNameCallback
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.MembershipCallback
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.PlaybackTimelineCallback
import uniffi.bae_bridge.PlaybackValuesCallback
import uniffi.bae_bridge.PlaylistEntriesCallback
import uniffi.bae_bridge.PlaylistsCallback
import uniffi.bae_bridge.ProfilesCallback
import uniffi.bae_bridge.QueueCallback
import uniffi.bae_bridge.QueueUpcomingSubscription
import uniffi.bae_bridge.RatingsCallback
import uniffi.bae_bridge.ReleaseDetailSubscription
import uniffi.bae_bridge.StorageBrowseSubscription
import uniffi.bae_bridge.StreamFailureCallback
import uniffi.bae_bridge.SyncActivityCallback
import uniffi.bae_bridge.SyncStatusCallback
import uniffi.bae_bridge.UiEventCallback
import uniffi.bae_bridge.UploadsPausedCallback
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
    val liveSubscriptions = mutableListOf<FakeLiveSubscription>()
    val albumBrowseSubscriptions = mutableListOf<FakeAlbumBrowseSubscription>()
    val composerBrowseSubscriptions = mutableListOf<FakeComposerBrowseSubscription>()
    val albumDetailSubscriptions = mutableListOf<FakeAlbumDetailSubscription>()
    val searchSubscriptions = mutableListOf<FakeLibrarySearchSubscription>()

    private fun liveSubscription(): FakeLiveSubscription = FakeLiveSubscription().also(liveSubscriptions::add)

    override fun pause() {
        pauseCount++
    }

    override fun resume() {
        resumeCount++
    }

    override suspend fun savePlaybackState() {}

    override fun subscribeUiEvents(callback: UiEventCallback) {}

    override fun subscribeConfig(callback: ConfigCallback): LiveSubscription = liveSubscription()

    override fun subscribeSyncStatus(callback: SyncStatusCallback): LiveSubscription = liveSubscription()

    override fun subscribeSyncActivity(callback: SyncActivityCallback): LiveSubscription = liveSubscription()

    override fun subscribeDownloadCounts(callback: DownloadCountsCallback): LiveSubscription = liveSubscription()

    override fun subscribeDownloadBrowse(): DownloadBrowseSubscription = unfaked("subscribeDownloadBrowse")

    override fun feedDownloadTransfers(activity: KeyActivityCallback): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeUploadsPaused(callback: UploadsPausedCallback): LiveSubscription = liveSubscription()

    override fun feedReleaseUploadTransfers(
        activity: KeyActivityCallback,
        failure: StreamFailureCallback,
    ): KeyedValueStream = FakeKeyedValueStream()

    override fun feedUploadTotalTransfer(
        activity: KeyActivityCallback,
        failure: StreamFailureCallback,
    ): KeyedValueStream = FakeKeyedValueStream()

    override fun feedUploadFileShares(activity: KeyActivityCallback): KeyedValueStream = FakeKeyedValueStream()

    override fun feedPinFileShares(activity: KeyActivityCallback): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeCastDevices(callback: CastDevicesCallback): LiveSubscription = liveSubscription()

    override fun subscribeQueue(callback: QueueCallback): LiveSubscription = liveSubscription()

    override fun subscribePlaybackValues(callback: PlaybackValuesCallback): LiveSubscription = liveSubscription()

    override fun subscribePlaybackTimeline(callback: PlaybackTimelineCallback): LiveSubscription = liveSubscription()

    override fun feedPlaybackPositions(activity: KeyActivityCallback): KeyedValueStream = FakeKeyedValueStream()

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
        ).also(albumBrowseSubscriptions::add)

    override fun subscribeComposerBrowse(sortCriteria: List<BridgeComposerSortCriterion>): ComposerBrowseSubscription =
        FakeComposerBrowseSubscription(composerPages).also(composerBrowseSubscriptions::add)

    override fun subscribeAlbumDetail(): AlbumDetailSubscription =
        FakeAlbumDetailSubscription(FakeDetailRead { albumDetails[it] }).also(albumDetailSubscriptions::add)

    override fun subscribeComposerDetail(): ComposerDetailSubscription =
        FakeComposerDetailSubscription(FakeDetailRead { composerDetails[it] })

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

    override fun playRelease(
        releaseId: String,
        startTrackId: String?,
        shuffle: Boolean,
    ) {
        playReleaseCalls.add(Triple(releaseId, startTrackId, shuffle))
    }

    /** Closing the fake releases nothing. */
    override fun close() {}

    // Every other call the bridge offers: a test that reaches one fakes it here first.

    override fun closeLibrary() {
        unfaked("closeLibrary")
    }

    override suspend fun detectS3Settings(
        location: BridgeStorageLocation,
        credentials: BridgeS3Credentials,
    ): BridgeS3Detection = unfaked("detectS3Settings")

    override suspend fun storageLocation(): BridgeStorageLocation? = unfaked("storageLocation")

    override fun subscribeAlbumSelection(): AlbumSelectionSubscription = unfaked("subscribeAlbumSelection")

    override fun addNext(trackIds: List<String>) {
        unfaked("addNext")
    }

    override fun addReleaseNext(releaseId: String) {
        unfaked("addReleaseNext")
    }

    override fun addReleaseToQueue(releaseId: String) {
        unfaked("addReleaseToQueue")
    }

    override fun addToQueue(trackIds: List<String>) {
        unfaked("addToQueue")
    }

    override suspend fun cancelDownload(releaseId: String) {
        unfaked("cancelDownload")
    }

    override fun cancelSidePauseCountdown() {
        unfaked("cancelSidePauseCountdown")
    }

    override suspend fun changeCover(
        releaseId: String,
        selection: BridgeCoverSelection,
    ) {
        unfaked("changeCover")
    }

    override fun clearPlayingFrom() {
        unfaked("clearPlayingFrom")
    }

    override fun clearUpNext() {
        unfaked("clearUpNext")
    }

    override suspend fun cloudOnlyReleaseCount(): ULong = unfaked("cloudOnlyReleaseCount")

    override suspend fun deleteRelease(releaseId: String) {
        unfaked("deleteRelease")
    }

    override suspend fun detachReleaseAudio(
        releaseId: String,
        newPath: String,
    ) {
        unfaked("detachReleaseAudio")
    }

    override suspend fun disconnectCloudProvider() {
        unfaked("disconnectCloudProvider")
    }

    override suspend fun filePath(fileId: String): String? = unfaked("filePath")

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

    override fun insertInQueue(
        trackIds: List<String>,
        index: UInt,
    ) {
        unfaked("insertInQueue")
    }

    override fun nextTrack() {
        unfaked("nextTrack")
    }

    override fun playLibraryShuffled() {
        unfaked("playLibraryShuffled")
    }

    override fun playReleases(releaseIds: List<String>) {
        unfaked("playReleases")
    }

    override fun previewPlay(target: BridgePreviewTarget) {
        unfaked("previewPlay")
    }

    override fun previewSeekByRatio(ratio: Double) {
        unfaked("previewSeekByRatio")
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

    override fun removeEntry(entryId: String) {
        unfaked("removeEntry")
    }

    override fun reorderEntry(
        entryId: String,
        beforeEntryId: String?,
    ) {
        unfaked("reorderEntry")
    }

    override suspend fun resolveToTrackIds(ids: List<String>): List<String> = unfaked("resolveToTrackIds")

    override suspend fun retryDownloads() {
        unfaked("retryDownloads")
    }

    override suspend fun saveSyncConfig(
        configData: BridgeSaveSyncConfig,
        deviceName: String,
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

    override suspend fun setPrimaryRelease(
        albumId: String,
        releaseId: String,
    ) {
        unfaked("setPrimaryRelease")
    }

    override fun setRepeatMode(mode: BridgeRepeatMode) {
        unfaked("setRepeatMode")
    }

    override fun setShuffle(on: Boolean) {
        unfaked("setShuffle")
    }

    override fun setVolume(volume: Float) {
        unfaked("setVolume")
    }

    override fun skipToEntry(entryId: String) {
        unfaked("skipToEntry")
    }

    override fun stop() {
        unfaked("stop")
    }

    override suspend fun unpinRelease(releaseId: String) {
        unfaked("unpinRelease")
    }

    override suspend fun uploadReleases(releaseIds: List<String>): BridgeUploadReleasesOutcome = unfaked("uploadReleases")

    override suspend fun fetchReleaseImageBytes(
        releaseId: String,
        source: BridgeGallerySource,
    ): ByteArray = unfaked("fetchReleaseImageBytes")

    override fun subscribeArtistBrowse(sortCriteria: List<BridgeArtistSortCriterion>): ArtistBrowseSubscription =
        unfaked("subscribeArtistBrowse")

    override fun getConfig(): BridgeConfig = unfaked("getConfig")

    override suspend fun lockActiveLibrary() {
        unfaked("lockActiveLibrary")
    }

    override suspend fun renameLibrary(name: String) {
        unfaked("renameLibrary")
    }

    override suspend fun setDefaultReleaseSavePreset(presetId: String) {
        unfaked("setDefaultReleaseSavePreset")
    }

    override suspend fun setDefaultTrackSavePreset(presetId: String) {
        unfaked("setDefaultTrackSavePreset")
    }

    override suspend fun setIdentifyAutomatically(enabled: Boolean) {
        unfaked("setIdentifyAutomatically")
    }

    override suspend fun setImportPinned(enabled: Boolean) {
        unfaked("setImportPinned")
    }

    override suspend fun setImportToCloud(enabled: Boolean) {
        unfaked("setImportToCloud")
    }

    override suspend fun setImportWhenIdentified(enabled: Boolean) {
        unfaked("setImportWhenIdentified")
    }

    override suspend fun setLibraryFullWidth(enabled: Boolean) {
        unfaked("setLibraryFullWidth")
    }

    override suspend fun setMaxConcurrentDownloads(n: UInt) {
        unfaked("setMaxConcurrentDownloads")
    }

    override suspend fun setMaxConcurrentUploads(n: UInt) {
        unfaked("setMaxConcurrentUploads")
    }

    override suspend fun setMetadataSourceEnabled(
        source: BridgeCatalog,
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

    override fun startCastDiscovery() {
        unfaked("startCastDiscovery")
    }

    override fun stopCastDiscovery() {
        unfaked("stopCastDiscovery")
    }

    override fun stopCasting() {
        unfaked("stopCasting")
    }

    override fun subscribeArtistDetail(): ArtistDetailSubscription = unfaked("subscribeArtistDetail")

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
        name: String,
        access: BridgeInviteAccess,
        delivery: BridgeInviteDelivery,
    ): BridgeInvite = unfaked("invitePerson")

    override suspend fun people(): List<BridgePerson> = unfaked("people")

    override suspend fun ownPerson(): BridgePerson? = unfaked("ownPerson")

    override suspend fun removeDevice(device: ULong): BridgeProviderSignOut? = unfaked("removeDevice")

    override suspend fun removeMember(member: String) {
        unfaked("removeMember")
    }

    override suspend fun setMemberRole(
        member: String,
        role: BridgeMemberRole,
    ) {
        unfaked("setMemberRole")
    }

    override fun subscribeInvites(callback: InvitesCallback): LiveSubscription = unfaked("subscribeInvites")

    override fun subscribeMembers(callback: MembershipCallback): LiveSubscription = unfaked("subscribeMembers")

    override fun subscribeAddedDevice(
        watch: BridgeAddedDeviceWatch,
        callback: AddedDeviceCallback,
    ): LiveSubscription = unfaked("subscribeAddedDevice")

    override suspend fun appendPlaylistEntry(
        playlist: String,
        track: String,
    ): String = unfaked("appendPlaylistEntry")

    override suspend fun avatar(person: String): ByteArray? = unfaked("avatar")

    override suspend fun createPlaylist(
        name: String,
        sharing: BridgeSharing,
    ): BridgePlaylist = unfaked("createPlaylist")

    override suspend fun createProfile(name: String): BridgePersonProfile = unfaked("createProfile")

    override suspend fun deletePlaylist(id: String) {
        unfaked("deletePlaylist")
    }

    override suspend fun libraryName(): String = unfaked("libraryName")

    override suspend fun playlistEntries(id: String): List<BridgePlaylistEntry> = unfaked("playlistEntries")

    override suspend fun playlists(): List<BridgePlaylist> = unfaked("playlists")

    override suspend fun ratings(): List<BridgeRating> = unfaked("ratings")

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

    override suspend fun setHistorySharing(sharing: BridgeSharing) {
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

    override fun subscribeLibraryName(callback: LibraryNameCallback): LiveSubscription = unfaked("subscribeLibraryName")

    override fun subscribePlaylistEntries(
        id: String,
        callback: PlaylistEntriesCallback,
    ): LiveSubscription = unfaked("subscribePlaylistEntries")

    override fun subscribePlaylists(callback: PlaylistsCallback): LiveSubscription = unfaked("subscribePlaylists")

    override fun subscribeProfiles(callback: ProfilesCallback): LiveSubscription = unfaked("subscribeProfiles")

    override fun subscribeRatings(callback: RatingsCallback): LiveSubscription = unfaked("subscribeRatings")

    override fun subscribeQueueUpcoming(): QueueUpcomingSubscription = unfaked("subscribeQueueUpcoming")

    override suspend fun removeLibrary(): BridgeLibraryRemoval = unfaked("removeLibrary")

    override suspend fun unsentBeforeRemoval(): BridgeUnsentWork = unfaked("unsentBeforeRemoval")

    override fun subscribeStorageBrowse(
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
    ): StorageBrowseSubscription = unfaked("subscribeStorageBrowse")

    override suspend fun cancelReleaseTransition(releaseId: String) {
        unfaked("cancelReleaseTransition")
    }

    override suspend fun getUploadsPaused(): Boolean = unfaked("getUploadsPaused")

    override fun getSyncStatus(): BridgeSyncStatusSnapshot = unfaked("getSyncStatus")

    override fun isSyncReady(): Boolean = unfaked("isSyncReady")

    override suspend fun reconnectSync() {
        unfaked("reconnectSync")
    }

    override fun reportNetworkChange() {
        unfaked("reportNetworkChange")
    }

    override suspend fun retryNow(): BridgeSyncRetryOutcome = unfaked("retryNow")

    override suspend fun retryOutbox() {
        unfaked("retryOutbox")
    }

    override suspend fun retryPendingOperation(id: Long) {
        unfaked("retryPendingOperation")
    }

    override suspend fun cancelFailedReleaseMove(
        releaseId: String,
        id: Long,
    ) {
        unfaked("cancelFailedReleaseMove")
    }

    override suspend fun setSyncPaused(paused: Boolean) {
        unfaked("setSyncPaused")
    }
}

/** A stream nothing feeds, which holds nothing. */
internal class FakeKeyedValueStream : KeyedValueStream(NoHandle) {
    override fun latest(key: String): BridgeFastValue? = null

    override fun cancel() = Unit
}

package fm.bae.app.playback

import fm.bae.app.BridgeFixtures
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import uniffi.bae_bridge.AddedDeviceSubscription
import uniffi.bae_bridge.AdminChangesSubscription
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailSubscription
import uniffi.bae_bridge.AlbumSelectionSubscription
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.ArtistBrowseSubscription
import uniffi.bae_bridge.ArtistDetailSubscription
import uniffi.bae_bridge.ArtistSearchSubscription
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddedDeviceWatch
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCastDevice
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeGallerySource
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteLifetime
import uniffi.bae_bridge.BridgeJoinRequest
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeLookupCatalog
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackTimeline
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgePlaylist
import uniffi.bae_bridge.BridgePlaylistEntry
import uniffi.bae_bridge.BridgePreviewTarget
import uniffi.bae_bridge.BridgeProviderSignOut
import uniffi.bae_bridge.BridgeQueueAddition
import uniffi.bae_bridge.BridgeRating
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeRendererService
import uniffi.bae_bridge.BridgeRendererServiceType
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeReportedRenderer
import uniffi.bae_bridge.BridgeRestoreCodeShare
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
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeUnsentWork
import uniffi.bae_bridge.BridgeUploadReleasesOutcome
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.CastDevicesSubscription
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailRowsSubscription
import uniffi.bae_bridge.ComposerDetailSubscription
import uniffi.bae_bridge.ConfigSubscription
import uniffi.bae_bridge.DownloadBrowseSubscription
import uniffi.bae_bridge.DownloadCountsSubscription
import uniffi.bae_bridge.InvitesSubscription
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.LibraryNameSubscription
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.MembersSubscription
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.PlaybackTimelineSubscription
import uniffi.bae_bridge.PlaybackValuesSubscription
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

    override fun subscribeDownloadBrowse(): DownloadBrowseSubscription = unfaked("subscribeDownloadBrowse")

    override fun feedDownloadTransfers(): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeUploadsPaused(): UploadsPausedSubscription = FakeUploadsPausedSubscription()

    override fun subscribeUnmeteredOnly(): UnmeteredOnlySubscription = FakeUnmeteredOnlySubscription()

    override fun feedReleaseUploadTransfers(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedUploadTotalTransfer(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedUploadFileShares(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedPinFileShares(): KeyedValueStream = FakeKeyedValueStream()

    override fun subscribeCastDevices(): CastDevicesSubscription = FakeCastDevicesSubscription()

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

    /** Closing the fake releases nothing. */
    override fun close() {}

    // Every other call the bridge offers: a test that reaches one fakes it here first.

    override fun closeLibrary(closing: BridgeLibraryClosing?) {
        unfaked("closeLibrary")
    }

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

    override suspend fun clearPlayingFrom() {
        unfaked("clearPlayingFrom")
    }

    override suspend fun clearUpNext() {
        unfaked("clearUpNext")
    }

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

    override fun insertInQueue(
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

    override suspend fun playTracks(
        trackIds: List<String>,
        startIndex: UInt,
    ): BridgePlayOutcome = unfaked("playTracks")

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

    override suspend fun removeEntry(entryId: String) {
        unfaked("removeEntry")
    }

    override suspend fun reorderEntry(
        entryId: String,
        beforeEntryId: String?,
    ): ULong = unfaked("reorderEntry")

    override suspend fun resolveToTrackIds(ids: List<String>): List<String> = unfaked("resolveToTrackIds")

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
        lifetime: BridgeInviteLifetime,
    ): String = unfaked("invitePerson")

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

    /** This device's open invites the handle answers, which a test delivers to. */
    val invites = FakeInvitesSubscription()

    override fun subscribeInvites(): InvitesSubscription = invites

    /** The admin changes the handle answers, which a test delivers to. */
    val adminChanges = FakeAdminChangesSubscription()

    override fun subscribeAdminChanges(): AdminChangesSubscription = adminChanges

    override suspend fun acknowledgeAdminChange(operation: Long) {
        unfaked("acknowledgeAdminChange")
    }

    override fun subscribeMembers(): MembersSubscription = unfaked("subscribeMembers")

    override fun subscribeAddedDevice(watch: BridgeAddedDeviceWatch): AddedDeviceSubscription = unfaked("subscribeAddedDevice")

    override suspend fun appendPlaylistEntry(
        playlist: String,
        track: String,
    ): String = unfaked("appendPlaylistEntry")

    override suspend fun avatar(person: String): ByteArray? = unfaked("avatar")

    override suspend fun createPlaylist(
        name: String,
        sharing: BridgeSharing,
    ): BridgePlaylist = unfaked("createPlaylist")

    override suspend fun createProfile(name: String) {
        unfaked("createProfile")
    }

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

    override fun subscribeLibraryName(): LibraryNameSubscription = unfaked("subscribeLibraryName")

    override fun subscribeProfiles(): ProfilesSubscription = unfaked("subscribeProfiles")

    /** The profile this device's person asked for, which a test delivers to. */
    val requestedProfile = FakeRequestedProfileSubscription()

    override fun subscribeRequestedProfile(): RequestedProfileSubscription = requestedProfile

    override fun subscribeQueueUpcoming(): QueueUpcomingSubscription = unfaked("subscribeQueueUpcoming")

    override suspend fun removeLibrary(closing: BridgeLibraryClosing?): BridgeLibraryRemoval = unfaked("removeLibrary")

    override suspend fun unsentBeforeRemoval(): BridgeUnsentWork = unfaked("unsentBeforeRemoval")

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

/** A stream nothing feeds, which holds nothing: each pull waits until it is cancelled. */
internal class FakeKeyedValueStream : KeyedValueStream(NoHandle) {
    private val cancelled = CompletableDeferred<Unit>()

    override fun latest(key: ByteArray): BridgeFastValue? = null

    override suspend fun next(): List<ByteArray> {
        cancelled.await()
        throw BridgeException.Cancelled()
    }

    override fun cancel() {
        cancelled.complete(Unit)
    }
}

/** A cast device list nothing delivers: each pull waits until cancelled. */
internal class FakeCastDevicesSubscription : CastDevicesSubscription(NoHandle) {
    private val devices = Channel<List<BridgeCastDevice>>(Channel.UNLIMITED)

    override suspend fun next(): List<BridgeCastDevice> = devices.receive()

    override suspend fun cancel() {
        devices.close(BridgeException.Cancelled())
    }

    override fun close() {
        devices.close(BridgeException.Cancelled())
    }
}

/** A sync status a test hands the app a value at a time: each pull waits for the next. */
internal class FakeSyncStatusSubscription : SyncStatusSubscription(NoHandle) {
    private val values = Channel<BridgeSyncStatusSnapshot>(Channel.UNLIMITED)

    fun deliver(status: BridgeSyncStatusSnapshot) {
        values.trySend(status)
    }

    override suspend fun next(): BridgeSyncStatusSnapshot = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** A sync activity a test hands the app a value at a time: each pull waits for the next. */
internal class FakeSyncActivitySubscription : SyncActivitySubscription(NoHandle) {
    private val values = Channel<BridgeSyncActivity>(Channel.UNLIMITED)

    fun deliver(activity: BridgeSyncActivity) {
        values.trySend(activity)
    }

    override suspend fun next(): BridgeSyncActivity = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Admin changes a test hands the app a list at a time: each pull waits for the next. */
internal class FakeAdminChangesSubscription : AdminChangesSubscription(NoHandle) {
    private val values = Channel<List<BridgeAdminChange>>(Channel.UNLIMITED)

    fun deliver(changes: List<BridgeAdminChange>) {
        values.trySend(changes)
    }

    override suspend fun next(): List<BridgeAdminChange> = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Open invites a test hands the app a list at a time: each pull waits for the next. */
internal class FakeInvitesSubscription : InvitesSubscription(NoHandle) {
    private val values = Channel<List<BridgeInvite>>(Channel.UNLIMITED)

    fun deliver(invites: List<BridgeInvite>) {
        values.trySend(invites)
    }

    override suspend fun next(): List<BridgeInvite> = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Preferences nothing delivers: each pull waits until cancelled. */
internal class FakeConfigSubscription : ConfigSubscription(NoHandle) {
    private val values = Channel<BridgeConfig>(Channel.UNLIMITED)

    override suspend fun next(): BridgeConfig = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Playback values a test hands the app a value at a time: each pull waits for the next. */
internal class FakePlaybackValuesSubscription : PlaybackValuesSubscription(NoHandle) {
    private val values = Channel<BridgePlaybackValues>(Channel.UNLIMITED)

    fun deliver(playback: BridgePlaybackValues) {
        values.trySend(playback)
    }

    override suspend fun next(): BridgePlaybackValues = values.receive()

    override suspend fun cancel() {
        values.close(BridgeException.Cancelled())
    }

    override fun close() {
        values.close(BridgeException.Cancelled())
    }
}

/** Queue additions nothing delivers: each pull waits until cancelled. */
internal class FakeQueueAdditionsSubscription : QueueAdditionsSubscription(NoHandle) {
    private val additions = Channel<BridgeQueueAddition>(Channel.UNLIMITED)

    override suspend fun next(): BridgeQueueAddition = additions.receive()

    override suspend fun cancel() {
        additions.close(BridgeException.Cancelled())
    }

    override fun close() {
        additions.close(BridgeException.Cancelled())
    }
}

/** Timelines a test hands the app one at a time: each pull waits for the next, or answers a failure a test queued. */
internal class FakePlaybackTimelineSubscription : PlaybackTimelineSubscription(NoHandle) {
    private val pulls = Channel<Result<BridgePlaybackTimeline>>(Channel.UNLIMITED)

    fun deliver(timeline: BridgePlaybackTimeline) {
        pulls.trySend(Result.success(timeline))
    }

    /** The next pull fails with [error]; the pull after it waits again. */
    fun fail(error: BridgeException) {
        pulls.trySend(Result.failure(error))
    }

    override suspend fun next(): BridgePlaybackTimeline = pulls.receive().getOrThrow()

    override suspend fun cancel() {
        pulls.close(BridgeException.Cancelled())
    }

    override fun close() {
        pulls.close(BridgeException.Cancelled())
    }
}

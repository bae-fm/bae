package fm.bae.app.playback

import fm.bae.app.BridgeFixtures
import kotlinx.coroutines.awaitCancellation
import uniffi.bae_bridge.AddDeviceStatusSubscription
import uniffi.bae_bridge.AdminChangesSubscription
import uniffi.bae_bridge.AirPlayCodePromptCallback
import uniffi.bae_bridge.AlbumDetailSubscription
import uniffi.bae_bridge.AlbumPlaceSubscription
import uniffi.bae_bridge.AlbumSectionsSubscription
import uniffi.bae_bridge.AlbumSelectionSubscription
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.ArtistDetailSubscription
import uniffi.bae_bridge.BridgeAccessKeyOutcome
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeAddDeviceRequest
import uniffi.bae_bridge.BridgeAdminChangePlace
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumPlace
import uniffi.bae_bridge.BridgeAlbumSearchResult
import uniffi.bae_bridge.BridgeBrowseTrack
import uniffi.bae_bridge.BridgeBrowsingFor
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeCancellation
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeConfig
import uniffi.bae_bridge.BridgeCopyReset
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
import uniffi.bae_bridge.BridgeList
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeLookupCatalog
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeOutboxFeeds
import uniffi.bae_bridge.BridgePagedList
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePreviewTarget
import uniffi.bae_bridge.BridgeQueueLane
import uniffi.bae_bridge.BridgeRatingTarget
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.BridgeSaveFilenamePiece
import uniffi.bae_bridge.BridgeSaveFolderPiece
import uniffi.bae_bridge.BridgeSavePreset
import uniffi.bae_bridge.BridgeSavePresetWrite
import uniffi.bae_bridge.BridgeSaveSyncConfig
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSetupCancellation
import uniffi.bae_bridge.BridgeSidePauseCountdown
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeStorageFilter
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeStorageSort
import uniffi.bae_bridge.BridgeStoreKeyState
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.BridgeTopPlayable
import uniffi.bae_bridge.BridgeUploadReleasesOutcome
import uniffi.bae_bridge.BridgeWorkChild
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.CastReceiversSubscription
import uniffi.bae_bridge.ComposerDetailSubscription
import uniffi.bae_bridge.ConfigSubscription
import uniffi.bae_bridge.DeviceStorageSubscription
import uniffi.bae_bridge.DownloadCountsSubscription
import uniffi.bae_bridge.InvitesSubscription
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.LibraryNameSubscription
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.ListCountSubscription
import uniffi.bae_bridge.ListPageSubscription
import uniffi.bae_bridge.ListViewSubscription
import uniffi.bae_bridge.MembersSubscription
import uniffi.bae_bridge.OpenInviteSubscription
import uniffi.bae_bridge.PeopleSubscription
import uniffi.bae_bridge.PersonSubscription
import uniffi.bae_bridge.PlaybackTimelineSubscription
import uniffi.bae_bridge.PlaybackValuesSubscription
import uniffi.bae_bridge.QueueSubscription
import uniffi.bae_bridge.ReleaseDetailSubscription
import uniffi.bae_bridge.SavePresetSubscription
import uniffi.bae_bridge.SessionUpNextSubscription
import uniffi.bae_bridge.StorageQueuesSubscription
import uniffi.bae_bridge.StorageSelectionSubscription
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
    val albumList: FakeAlbumList = FakeAlbumList(),
    private val albumDetails: Map<String, BridgeAlbumDetail> = emptyMap(),
    private val composerDetailRows: Map<String, List<BridgeComposerDetailRow>> = emptyMap(),
    private val workChildren: Map<String, List<BridgeWorkChild>> = emptyMap(),
    private val albumTracks: Map<String, List<BridgeBrowseTrack>> = emptyMap(),
    private val releaseDetails: Map<String, BridgeRelease> = emptyMap(),
    private val searchResults: (query: String) -> BridgeSearchResults = { BridgeFixtures.searchResults() },
    private val searchTopPlayable: (query: String) -> BridgeTopPlayable? = { null },
    private val initialSearchError: (query: String) -> uniffi.bae_bridge.BridgeException? = { null },
    var deliverSearchResultsImmediately: Boolean = true,
) : PlaylistFakeAppHandle() {
    private val composerList = FakeComposerList()

    var pauseCount = 0
    var resumeCount = 0
    var playbackFailureDismissals = 0

    val playReleaseCalls = mutableListOf<Triple<String, String?, Boolean>>()
    val playTracksCalls = mutableListOf<Pair<List<String>, UInt>>()
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

    override fun subscribeSavePreset(id: String): SavePresetSubscription = unfaked("subscribeSavePreset")

    override suspend fun getConfig(): BridgeConfig = unfaked("getConfig")

    /** The sync status the handle answers, which a test delivers to. */
    val syncStatus = FakeSyncStatusSubscription()

    override fun subscribeSyncStatus(): SyncStatusSubscription = syncStatus

    /** The sync activity the handle answers, which a test delivers to. */
    val syncActivity = FakeSyncActivitySubscription()

    override fun subscribeSyncActivity(): SyncActivitySubscription = syncActivity

    override fun subscribeDownloadCounts(): DownloadCountsSubscription = FakeDownloadCountsSubscription()

    override fun subscribeDeviceStorage(): DeviceStorageSubscription = unfaked("subscribeDeviceStorage")

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

    override fun subscribeSessionUpNext(limit: UInt): SessionUpNextSubscription = FakeSessionUpNextSubscription()

    /** The playback values the handle answers, which a test delivers to. */
    val playbackValues = FakePlaybackValuesSubscription()

    override fun subscribePlaybackValues(): PlaybackValuesSubscription = playbackValues

    /** The playback timeline the handle answers, which a test delivers to. */
    val playbackTimeline = FakePlaybackTimelineSubscription()

    override fun subscribePlaybackTimeline(): PlaybackTimelineSubscription = playbackTimeline

    override fun feedPlaybackPositions(): KeyedValueStream = FakeKeyedValueStream()

    override fun feedTrackBuffered(): KeyedValueStream = FakeKeyedValueStream()

    override fun triggerSync() {}

    override suspend fun fetchLibraryImageBytes(image: BridgeImageRef): ByteArray? = null

    override fun subscribeAlbumSections(sortCriteria: List<BridgeSortCriterion>): AlbumSectionsSubscription =
        unfaked("subscribeAlbumSections")

    override fun subscribeListView(
        list: BridgeList,
        first: ULong,
        last: ULong,
    ): ListViewSubscription = fakeList(list).openView(first, last)

    override fun subscribeListCount(list: BridgeList): ListCountSubscription = fakeList(list).openCount()

    override fun subscribeListPage(
        list: BridgePagedList,
        offset: ULong,
        limit: ULong,
    ): ListPageSubscription = fakePagedList(list).openPage(offset, limit)

    override fun subscribeListPageCount(list: BridgePagedList): ListCountSubscription = fakePagedList(list).openCount()

    /** The paged list [list] names, which a test changes and delivers. */
    fun fakePagedList(list: BridgePagedList): FakeList<*> =
        when (list) {
            is BridgePagedList.AlbumGrid -> albumList
            is BridgePagedList.Composers -> composerList
            is BridgePagedList.ComposerPage -> composerPage(list.artistId)
            is BridgePagedList.AlbumTracks -> albumTrackList(list.albumId)
            is BridgePagedList.WorkChildren -> workChildList(list.workId)
            is BridgePagedList.SearchAlbums -> searchAlbumList(list.query)
        }

    private val composerPageLists = mutableMapOf<String, FakeComposerPage>()
    private val albumTrackLists = mutableMapOf<String, FakeList<BridgeBrowseTrack>>()
    private val workChildLists = mutableMapOf<String, FakeList<BridgeWorkChild>>()
    private val searchAlbumLists = mutableMapOf<String, FakeList<BridgeAlbumSearchResult>>()

    /** The list [list] names, which a test changes and delivers. */
    fun fakeList(list: BridgeList): FakeList<*> =
        when (list) {
            is BridgeList.AlbumGrid -> {
                albumList
            }

            is BridgeList.Composers -> {
                composerList
            }

            is BridgeList.ComposerPage -> {
                composerPage(list.artistId)
            }

            is BridgeList.Artists,
            BridgeList.Downloads,
            is BridgeList.Upcoming,
            is BridgeList.ArtistSearch,
            is BridgeList.Storage,
            -> {
                unfaked("the list $list")
            }
        }

    override fun subscribeStorageSelection(
        filter: BridgeStorageFilter,
        search: String,
    ): StorageSelectionSubscription = unfaked("subscribeStorageSelection")

    override suspend fun storageIdsBetween(
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        search: String,
        from: String,
        to: String,
    ): List<String> = unfaked("storageIdsBetween")

    override suspend fun storageIds(
        filter: BridgeStorageFilter,
        search: String,
    ): List<String> = unfaked("storageIds")

    override fun subscribeStorageQueues(): StorageQueuesSubscription = unfaked("subscribeStorageQueues")

    override fun subscribeAlbumDetail(): AlbumDetailSubscription =
        FakeAlbumDetailSubscription(FakeDetailRead { albumDetails[it.albumId] }).also(albumDetailSubscriptions::add)

    override fun subscribeComposerDetail(): ComposerDetailSubscription = FakeComposerDetailSubscription(FakeDetailRead { null })

    override fun subscribeWorkDetail(): WorkDetailSubscription = FakeWorkDetailSubscription(FakeDetailRead { null })

    override fun subscribeReleaseDetail(): ReleaseDetailSubscription = FakeReleaseDetailSubscription(FakeDetailRead { releaseDetails[it] })

    override fun subscribeLibrarySearch(): LibrarySearchSubscription =
        FakeLibrarySearchSubscription { query ->
            if (!deliverSearchResultsImmediately) {
                null
            } else {
                initialSearchError(query)?.let { Result.failure(it) } ?: Result.success(searchResults(query))
            }
        }.also(searchSubscriptions::add)

    /** The composer [artistId]'s page rows, which a test changes and delivers. */
    private fun composerPage(artistId: String): FakeComposerPage =
        composerPageLists.getOrPut(artistId) { FakeComposerPage(composerDetailRows[artistId].orEmpty()) }

    /** The work [workId]'s children as the bridge lists them. */
    private fun workChildList(workId: String): FakeList<BridgeWorkChild> =
        workChildLists.getOrPut(workId) {
            FakeList(workChildren[workId].orEmpty(), ::workChildId, { BridgeListRows.WorkChildren(it) })
        }

    /** The album [albumId]'s tracks as the bridge lists them, which a test changes and delivers. */
    fun albumTrackList(albumId: String): FakeList<BridgeBrowseTrack> =
        albumTrackLists.getOrPut(albumId) {
            FakeList(albumTracks[albumId].orEmpty(), { it.id }, { BridgeListRows.AlbumTracks(it) })
        }

    /** A work's child as the fake work child list orders it: by the part's work id or the release's id. */
    private fun workChildId(child: BridgeWorkChild): String =
        when (child) {
            is BridgeWorkChild.Part -> child.work.workId
            is BridgeWorkChild.Release -> child.release.releaseId
        }

    /**
     * The albums a search for [query] lists as the bridge lists them, which a test changes and delivers: the
     * search's albums, whose count fails with the phrase's search error.
     */
    fun searchAlbumList(query: String): FakeList<BridgeAlbumSearchResult> =
        searchAlbumLists.getOrPut(query) {
            FakeList(
                searchResults(query).albums,
                { it.id },
                { BridgeListRows.SearchAlbums(it) },
                deliverImmediately = deliverSearchResultsImmediately,
                countFailure = initialSearchError(query),
            )
        }

    /** Returns the release's track with that id as a BridgeBrowseTrack, as the database would. */
    override suspend fun readReleaseTrack(
        releaseId: String,
        trackId: String,
    ): BridgeBrowseTrack? =
        releaseDetails[releaseId]?.let { release ->
            release.tracks.singleOrNull { it.id == trackId }?.let {
                BridgeBrowseTrack(it.id, it.title, it.durationMs, release.id, it.artistLine, release.cover)
            }
        }

    override suspend fun readSearchTopPlayable(query: String): BridgeTopPlayable? = searchTopPlayable(query)

    override suspend fun readAlbumDetail(albumId: String): BridgeAlbumDetail? = albumDetails[albumId]

    override suspend fun readComposerDetail(artistId: String): BridgeComposerDetail? = null

    override suspend fun readWorkDetail(workId: String): BridgeWorkDetail? = null

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

    override fun subscribeAlbumSelection(): AlbumSelectionSubscription = unfaked("subscribeAlbumSelection")

    override suspend fun addNext(trackIds: List<String>): UInt {
        unfaked("addNext")
    }

    override suspend fun addPickedNext(trackIds: List<String>): UInt {
        unfaked("addPickedNext")
    }

    override suspend fun addPickedToQueue(trackIds: List<String>): UInt {
        unfaked("addPickedToQueue")
    }

    override suspend fun addReleaseNext(releaseId: String): UInt {
        unfaked("addReleaseNext")
    }

    override suspend fun addReleaseToQueue(releaseId: String): UInt {
        unfaked("addReleaseToQueue")
    }

    override suspend fun addToQueue(trackIds: List<String>): UInt {
        unfaked("addToQueue")
    }

    override suspend fun cancelDownload(releaseId: String) {
        unfaked("cancelDownload")
    }

    override fun cancelSidePauseCountdown() {
        unfaked("cancelSidePauseCountdown")
    }

    override suspend fun dismissPlaybackFailure() {
        playbackFailureDismissals++
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

    override fun subscribeAlbumPlace(
        sortCriteria: List<BridgeSortCriterion>,
        albumId: String,
        groupByArtist: Boolean,
        sectionId: String?,
    ): AlbumPlaceSubscription = unfaked("subscribeAlbumPlace")

    override suspend fun albumIdsBetween(
        sortCriteria: List<BridgeSortCriterion>,
        groupByArtist: Boolean,
        fromAlbumId: String,
        fromSectionId: String?,
        toAlbumId: String,
        toSectionId: String?,
    ): List<String> = unfaked("albumIdsBetween")

    override suspend fun albumsInBrowseOrder(
        sortCriteria: List<BridgeSortCriterion>,
        albumIds: List<String>,
        groupByArtist: Boolean,
    ): List<BridgeAlbum> = unfaked("albumsInBrowseOrder")

    override suspend fun getDownloadCounts(): BridgeDownloadCounts = unfaked("getDownloadCounts")

    override suspend fun insertInQueue(
        trackIds: List<String>,
        index: UInt,
    ): UInt {
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
        lane: BridgeQueueLane,
        position: ULong,
    ): Boolean = unfaked("reorderEntry")

    override suspend fun playPickedTracks(trackIds: List<String>): BridgePlayOutcome = unfaked("playPickedTracks")

    override suspend fun trackIdsBetween(
        anchorId: String,
        targetId: String,
    ): List<String> = unfaked("trackIdsBetween")

    override suspend fun resolveToTrackIds(ids: List<String>): List<String> = unfaked("resolveToTrackIds")

    override suspend fun retryDownload(releaseId: String) {
        unfaked("retryDownload")
    }

    override suspend fun retryDownloads() {
        unfaked("retryDownloads")
    }

    override suspend fun saveSyncConfig(
        configData: BridgeSaveSyncConfig,
        setup: BridgeLibrarySetup?,
    ): BridgeStorageSetup = unfaked("saveSyncConfig")

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

    override suspend fun setVolume(volume: Float) {
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

    override suspend fun addSavePreset(preset: BridgeSavePreset): BridgeSavePresetWrite = unfaked("addSavePreset")

    override suspend fun replaceSavePreset(preset: BridgeSavePreset): BridgeSavePresetWrite = unfaked("replaceSavePreset")

    override suspend fun deleteSavePreset(id: String) {
        unfaked("deleteSavePreset")
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

    override suspend fun castTo(
        deviceId: String,
        prompt: AirPlayCodePromptCallback,
    ) {
        unfaked("castTo")
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

    override suspend fun invitePerson(
        role: BridgeMemberRole,
        access: BridgeInviteAccess,
        lifetime: BridgeInviteLifetime,
    ): String = unfaked("invitePerson")

    override fun holdInviteScreenSync(): BridgeInviteScreenHold = unfaked("holdInviteScreenSync")

    /** The people reads the app opened, newest last, which a test delivers to. */
    val peopleReads = mutableListOf<FakePeopleSubscription>()

    override fun subscribePeople(): PeopleSubscription = FakePeopleSubscription().also { peopleReads += it }

    /** The person subscriptions that `subscribePerson` opened, which a test delivers to. */
    val persons = mutableListOf<Pair<String, FakePersonSubscription>>()

    override fun subscribePerson(member: String): PersonSubscription = FakePersonSubscription().also { persons += member to it }

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

    /** The reads of the waiting join request the app opened, each with the invites it left out, newest last. */
    val waitingRequests = mutableListOf<Pair<List<String>, FakeOpenInviteSubscription>>()

    override fun subscribeWaitingJoinRequest(closed: List<String>): OpenInviteSubscription =
        FakeOpenInviteSubscription().also { waitingRequests += closed to it }

    /** The reads of one invite the app opened, each with its invite id, newest last. */
    val inviteReads = mutableListOf<Pair<String, FakeOpenInviteSubscription>>()

    override fun subscribeInvite(invite: String): OpenInviteSubscription = FakeOpenInviteSubscription().also { inviteReads += invite to it }

    /** The admin changes the handle answers, which a test delivers to. */
    val adminChanges = FakeAdminChangesSubscription()

    /** The places whose admin changes the app read, in order. */
    val adminChangePlaces = mutableListOf<BridgeAdminChangePlace>()

    override fun subscribeAdminChanges(place: BridgeAdminChangePlace): AdminChangesSubscription =
        adminChanges.also { adminChangePlaces += place }

    override suspend fun dismissRefusedInvite(operation: Long) {
        unfaked("dismissRefusedInvite")
    }

    override suspend fun acknowledgeAdminChange(operation: Long) {
        unfaked("acknowledgeAdminChange")
    }

    override fun subscribeMembers(): MembersSubscription = unfaked("subscribeMembers")

    override fun followAddDevice(pairing: String): AddDeviceStatusSubscription = unfaked("followAddDevice")

    override suspend fun avatar(person: String): ByteArray? = unfaked("avatar")

    override suspend fun libraryName(): String = unfaked("libraryName")

    override suspend fun renameProfile(name: String) {
        unfaked("renameProfile")
    }

    override suspend fun setAvatar(bytes: ByteArray?) {
        unfaked("setAvatar")
    }

    override suspend fun setHistorySharing(sharing: BridgeHistorySharing) {
        unfaked("setHistorySharing")
    }

    override suspend fun setRating(
        target: BridgeRatingTarget,
        stars: BridgeStars?,
    ) {
        unfaked("setRating")
    }

    override fun subscribeLibraryName(): LibraryNameSubscription = unfaked("subscribeLibraryName")

    override suspend fun removeLibrary(closing: BridgeLibraryClosing?): BridgeLibraryRemoval = unfaked("removeLibrary")

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

    override suspend fun useAccessKey(
        accessKeyId: String,
        secretAccessKey: String,
        cancellation: BridgeCancellation,
    ): BridgeAccessKeyOutcome = unfaked("useAccessKey")

    /**
     * A session waits for a copy reset from its start, and this copy is never behind: the wait ends only when the
     * session cancels it.
     */
    override suspend fun copyReset(): BridgeCopyReset = awaitCancellation()

    override suspend fun retryOutbox() {
        unfaked("retryOutbox")
    }

    override fun cancelStorageSetup(): BridgeSetupCancellation = unfaked("cancelStorageSetup")

    override suspend fun dismissUndoneStorageSetup(operation: Long) {
        unfaked("dismissUndoneStorageSetup")
    }

    override suspend fun setUnmeteredOnly(unmeteredOnly: Boolean) {
        unfaked("setUnmeteredOnly")
    }

    override suspend fun setSyncPaused(paused: Boolean) {
        unfaked("setSyncPaused")
    }
}

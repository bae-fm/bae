package fm.bae.app.playback

import android.content.Context
import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.Util
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import fm.bae.app.BaeLogger
import fm.bae.app.runLoggedBridgeCommand
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLoadingTrack
import uniffi.bae_bridge.BridgeNowPlayingTrack
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode

private const val TAG = "bae.BaeCorePlayer"
private val logger = BaeLogger(TAG)

/**
 * Media3 [Player] that mirrors bae-core's playback, which plays the audio itself,
 * so the [androidx.media3.session.MediaSession] and the in-app UI show what core
 * is doing. Transport commands go to core, and the value core publishes back
 * updates [State]; only an in-track seek shows its target before core confirms.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
class BaeCorePlayer(
    applicationLooper: Looper,
    private val appHandle: AppHandle,
    private val context: Context,
    private val scope: CoroutineScope,
    private val queueUpcomingSource: QueueUpcomingSource = QueueUpcomingSource(appHandle),
    /** Whether the app has a started Activity; Android refuses to start a service
     *  from the background, so [PlaybackSystemHooks] starts one only then. */
    private val isAppForeground: () -> Boolean,
) : SimpleBasePlayer(applicationLooper),
    PlaybackEventSink {
    internal val systemHooks =
        PlaybackSystemHooks(
            context = context,
            appHandle = appHandle,
            isAppForeground = isAppForeground,
            isPlaying = { _isPlaying.value },
            hasCurrentTrack = { currentMeta != null },
        )

    /** Display metadata for one track, from a queue entry or, for the current
     *  track, from the playback value. */
    internal data class Meta(
        /** Null for the current track's metadata, which is not a queue entry. */
        val entryId: String?,
        val trackId: String,
        val title: String,
        val artist: String,
        val albumTitle: String,
        /** Null when core reports no length, and for the current track's metadata. */
        val durationClock: BridgeDurationClock?,
        val coverImage: BridgeImageRef?,
    )

    /** The release or library the queue plays from, and its not-yet-played tracks. */
    internal data class ContextLane(
        val kind: BridgePlaybackSourceKind,
        val shuffled: Boolean,
        /** Only the first page of the [upcomingTotal] tracks. */
        val entries: List<Meta>,
        val upcomingTotal: Int,
    )

    internal companion object {
        /**
         * The MediaSession playlist: the current track, then [entries], which
         * exclude it. A queue entry with the current track's id is replaced by
         * [current] instead.
         */
        fun orderedMetas(
            entries: List<Meta>,
            current: Meta?,
        ): List<Meta> {
            val mapped = entries.map { if (it.trackId == current?.trackId) current else it }
            return if (current != null && mapped.none { it.trackId == current.trackId }) {
                listOf(current) + mapped
            } else {
                mapped
            }
        }

        /**
         * An empty playlist is always [Player.STATE_IDLE]: [SimpleBasePlayer]
         * crashes on an empty playlist in STATE_BUFFERING, which is where a load
         * sits before the track's metadata arrives.
         */
        fun playbackStateFor(
            transport: Transport,
            playlistIsEmpty: Boolean,
        ): Int =
            if (playlistIsEmpty) {
                Player.STATE_IDLE
            } else {
                when (transport) {
                    Transport.IDLE -> Player.STATE_IDLE
                    Transport.BUFFERING -> Player.STATE_BUFFERING
                    Transport.READY -> Player.STATE_READY
                }
            }

        internal fun itemDurationMs(
            meta: Meta,
            playingTrackId: String?,
            durationMs: Long,
        ): Long {
            if (meta.trackId != playingTrackId) return C.TIME_UNSET
            if (durationMs == C.TIME_UNSET) {
                logger.warning("itemDurationMs missing duration for current track ${meta.trackId}")
            }
            return durationMs
        }

        private fun mediaItemData(
            meta: Meta,
            playingTrackId: String?,
            durationMs: Long,
            artwork: ByteArray?,
        ): MediaItemData {
            val currentDurationMs = itemDurationMs(meta, playingTrackId, durationMs)
            val metadata =
                mediaMetadata(
                    meta,
                    if (currentDurationMs == C.TIME_UNSET) null else currentDurationMs,
                    artwork,
                )
            return MediaItemData
                .Builder(meta.trackId)
                .setMediaItem(
                    MediaItem
                        .Builder()
                        .setMediaId(meta.trackId)
                        .setMediaMetadata(metadata)
                        .build(),
                ).setMediaMetadata(metadata)
                .setDurationUs(
                    if (currentDurationMs == C.TIME_UNSET) C.TIME_UNSET else Util.msToUs(currentDurationMs),
                ).build()
        }

        internal fun mediaMetadata(
            meta: Meta,
            durationMs: Long?,
            artwork: ByteArray?,
        ): MediaMetadata {
            val metadataBuilder =
                MediaMetadata
                    .Builder()
                    .setTitle(meta.title)
                    .setArtist(meta.artist)
                    .setAlbumTitle(meta.albumTitle)
                    .setDurationMs(durationMs)
                    .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                    .setIsPlayable(true)
                    .setIsBrowsable(false)
            // Only the current track has artwork bytes (see getState).
            artwork?.let {
                metadataBuilder.setArtworkData(it, MediaMetadata.PICTURE_TYPE_FRONT_COVER)
            }
            return metadataBuilder.build()
        }
    }

    internal enum class Transport { IDLE, BUFFERING, READY }

    private var transport: Transport = Transport.IDLE
    private var playWhenReady: Boolean = false

    /** The up-next tracks in play order ([manualEntries] then the context's), for
     *  the Media3 playlist. */
    private var entries: List<Meta> = emptyList()

    /** The queue's two sections, kept apart for the in-app queue. */
    private var manualEntries: List<Meta> = emptyList()
    private var contextLane: ContextLane? = null

    /** Context tracks read past [ContextLane.entries], keyed by index in the whole tail; empty
     *  until a read for [queueRevision] arrives. */
    private var pagedUpcoming: Map<Int, QueueItem> = emptyMap()

    /** The revision [manualEntries] and [contextLane] came from. */
    private var queueRevision: ULong = 0u

    private val upcoming =
        QueueUpcomingWindows(queueUpcomingSource, scope) { revision ->
            if (revision == queueRevision) {
                pagedUpcoming = upcomingItems()
                publish()
            }
        }

    /** The current track's metadata, from the latest playback value. */
    private var currentMeta: Meta? = null

    /** The cover the current track's artwork is fetched for, and its bytes once
     *  they arrive. */
    private var currentArtworkCover: BridgeImageRef? = null
    private var currentArtwork: ByteArray? = null

    private var sidePausePrompt: SidePausePrompt? = null

    /** Null when stopped. */
    private var playingTrackId: String? = null
    private var lastSeekRevision: ULong = 0u
    private var hasNext: Boolean = false
    private var hasPrevious: Boolean = false
    private var media3RepeatMode: Int = Player.REPEAT_MODE_OFF

    private val positionModel = PlaybackPositionModel()

    private val _nowPlaying = MutableStateFlow<NowPlaying?>(null)
    val nowPlaying: StateFlow<NowPlaying?> = _nowPlaying.asStateFlow()

    private val _isPlaying = MutableStateFlow(false)
    val isPlaying: StateFlow<Boolean> = _isPlaying.asStateFlow()

    // True while core is loading or buffering a track.
    private val _isLoading = MutableStateFlow(false)
    val isLoading: StateFlow<Boolean> = _isLoading.asStateFlow()

    private val _position = MutableStateFlow(PlaybackPosition(0.0, null, null))
    val position: StateFlow<PlaybackPosition> = _position.asStateFlow()

    // The in-app queue; the current track is not in it.
    private val _queue = MutableStateFlow(QueueProjection.EMPTY)
    val queue: StateFlow<QueueProjection> = _queue.asStateFlow()

    private val _repeatMode = MutableStateFlow(BridgeRepeatMode.OFF)
    val repeatMode: StateFlow<BridgeRepeatMode> = _repeatMode.asStateFlow()

    // Volume in [0,1].
    private val _volume = MutableStateFlow(1f)
    val volume: StateFlow<Float> = _volume.asStateFlow()

    private val _isMuted = MutableStateFlow(false)
    val isMuted: StateFlow<Boolean> = _isMuted.asStateFlow()

    // "N added to queue" events. No replay, so a collector that resubscribes
    // never shows an old one again.
    private val _queueItemsAdded =
        MutableSharedFlow<Int>(
            replay = 0,
            extraBufferCapacity = 8,
            onBufferOverflow = BufferOverflow.DROP_OLDEST,
        )
    val queueItemsAdded: SharedFlow<Int> = _queueItemsAdded.asSharedFlow()

    init {
        systemHooks.attach()
    }

    fun togglePlayPause() {
        if (_isPlaying.value) pause() else play()
    }

    // ── Values and events from core, on the main thread ──

    fun applyValues(values: BridgePlaybackValues) {
        when (val state = values.state) {
            BridgePlaybackValueState.Stopped -> {
                onStopped()
            }

            is BridgePlaybackValueState.Loading -> {
                onLoading(state.track)
            }

            is BridgePlaybackValueState.Playing -> {
                activate(Transport.READY, state.track)
            }

            is BridgePlaybackValueState.Paused -> {
                applyPaused(state)
            }
        }
        values.position?.let {
            if (values.seekRevision != lastSeekRevision) {
                onSeeked(it.trackId, it.positionMs, it.durationMs.toLong(), it.progress)
            } else {
                onProgress(it.trackId, it.positionMs, it.durationMs.toLong(), it.progress)
            }
        }
        lastSeekRevision = values.seekRevision
        media3RepeatMode =
            when (values.repeatMode) {
                BridgeRepeatMode.OFF -> Player.REPEAT_MODE_OFF
                BridgeRepeatMode.TRACK -> Player.REPEAT_MODE_ONE
                BridgeRepeatMode.CONTEXT -> Player.REPEAT_MODE_ALL
            }
        _repeatMode.value = values.repeatMode
        _volume.value = values.volume
        _isMuted.value = values.isMuted
        publish()
    }

    private fun onLoading(track: BridgeLoadingTrack) {
        // Showing the loading track right away lets Media3 post the notification
        // and start the service while the app is still on screen; once the screen
        // locks, Android may refuse the start. Until core has prepared it, the
        // prior track stays on screen.
        val prepared =
            when (track) {
                is BridgeLoadingTrack.Unprepared -> null
                is BridgeLoadingTrack.Prepared -> track.track
            }
        activate(Transport.BUFFERING, prepared)
    }

    /** Enter play-when-ready for [transport]; a null [track] keeps the prior track. */
    private fun activate(
        transport: Transport,
        track: BridgeNowPlayingTrack?,
    ) {
        this.transport = transport
        playWhenReady = true
        sidePausePrompt = null
        if (track != null) {
            makeCurrent(track)
        }
        publish()
        systemHooks.onPlaybackActivated()
    }

    /** Make [track] the current track. Its duration is 0 when unknown. */
    private fun makeCurrent(track: BridgeNowPlayingTrack) {
        val trackId = track.track.trackId
        positionModel.setActiveTrack(
            trackChanged = trackId != playingTrackId,
            rawDurationMs = track.track.durationMs.toLong(),
        )
        playingTrackId = trackId
        currentMeta =
            Meta(
                entryId = null,
                trackId = trackId,
                title = track.display.title,
                artist = track.display.artistNames,
                albumTitle = track.display.albumTitle,
                durationClock = null,
                coverImage = track.display.coverImage,
            )
        refreshArtwork(track.display.coverImage)
    }

    /**
     * Fetch the artwork bytes for [coverImage] unless they are already loaded. The
     * reference includes the image's version, so a replaced cover is fetched again.
     */
    private fun refreshArtwork(coverImage: BridgeImageRef?) {
        if (coverImage == currentArtworkCover) return
        currentArtworkCover = coverImage
        currentArtwork = null
        if (coverImage == null) {
            invalidateState()
            return
        }
        scope.launch {
            val bytes =
                try {
                    appHandle.fetchLibraryImageBytes(coverImage)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logger.error("Failed to load now-playing artwork ${coverImage.id}", e)
                    null
                }
            // The track may have changed while this fetched.
            if (currentArtworkCover == coverImage) {
                if (bytes == null) {
                    logger.warning("now-playing artwork bytes absent for ${coverImage.id}")
                }
                currentArtwork = bytes
                invalidateState()
            }
        }
    }

    private fun applyPaused(state: BridgePlaybackValueState.Paused) {
        transport = Transport.READY
        playWhenReady = false
        makeCurrent(state.track)
        sidePausePrompt =
            when (val reason = state.reason) {
                BridgePlaybackPauseReason.Manual -> null
                is BridgePlaybackPauseReason.SideEnded -> SidePausePrompt.of(reason.prompt)
            }
        publish()
    }

    private fun onStopped() {
        transport = Transport.IDLE
        playWhenReady = false
        playingTrackId = null
        currentMeta = null
        refreshArtwork(null)
        sidePausePrompt = null
        positionModel.reset()
        systemHooks.onPlaybackStopped()
        publish()
    }

    private fun onProgress(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ) {
        applyPositionUpdate(
            positionModel.onProgress(playingTrackId, trackId, positionMs, durationMs, progress),
            trackId,
        )
    }

    private fun onSeeked(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ) {
        applyPositionUpdate(
            positionModel.onSeeked(playingTrackId, trackId, positionMs, durationMs, progress),
            trackId,
        )
    }

    private fun applyPositionUpdate(
        update: PositionUpdate,
        trackId: String,
    ) {
        when (update) {
            PositionUpdate.Applied -> publish()
            PositionUpdate.HeldByPendingSeek -> Unit
            PositionUpdate.StaleTrack -> logStalePosition(trackId)
        }
    }

    private fun logStalePosition(trackId: String) {
        logger.warning("ignoring playback position for stale track $trackId; current track is $playingTrackId")
    }

    override fun onQueueItemsAdded(count: Int) {
        _queueItemsAdded.tryEmit(count)
    }

    fun onQueueValue(
        manual: List<BridgeQueueEntry>,
        context: BridgePlaybackContext?,
        hasNext: Boolean,
        hasPrevious: Boolean,
        revision: ULong,
    ) {
        val manualMetas = manual.map { it.toEntry() }
        val lane =
            context?.let {
                ContextLane(
                    kind = it.kind,
                    shuffled = it.shuffled,
                    entries = it.upcoming.map { entry -> entry.toEntry() },
                    upcomingTotal = it.upcomingTotal.toInt(),
                )
            }
        if (revision < queueRevision) {
            logger.warning("dropping queue value at revision $revision; revision $queueRevision is already applied")
            return
        }
        manualEntries = manualMetas
        contextLane = lane
        if (revision > queueRevision) {
            queueRevision = revision
            pagedUpcoming = upcomingItems()
        }
        entries = manualMetas + (lane?.entries ?: emptyList())
        this.hasNext = hasNext
        this.hasPrevious = hasPrevious
        publish()
    }

    /** Read `[offset, offset + limit)` of the context's tail. See [QueueUpcomingWindows.load]. */
    fun loadUpcomingRange(
        offset: Int,
        limit: Int,
    ) {
        val lane = contextLane ?: return
        upcoming.load(offset until minOf(offset + limit, lane.upcomingTotal))
    }

    /** The context tracks read for [queueRevision], by index in the whole tail. */
    private fun upcomingItems(): Map<Int, QueueItem> =
        upcoming
            .entriesAt(queueRevision)
            .mapNotNull { (index, entry) -> entry.toEntry().toQueueItem()?.let { index to it } }
            .toMap()

    /** Refresh the Media3 [State] and the in-app flows from the same fields. */
    private fun publish() {
        invalidateState()
        val meta = currentMeta
        _nowPlaying.value =
            meta?.let {
                NowPlaying(it.trackId, it.title, it.artist, it.coverImage, sidePausePrompt)
            }
        _isPlaying.value = transport == Transport.READY && playWhenReady
        _isLoading.value = transport == Transport.BUFFERING
        _position.value = positionModel.position(hasCurrentTrack = currentMeta != null)
        _queue.value =
            QueueProjection(
                manual = manualEntries.mapNotNull { it.toQueueItem() },
                context =
                    contextLane?.let { lane ->
                        QueueContext(
                            kind = lane.kind,
                            shuffled = lane.shuffled,
                            upcoming = lane.entries.mapNotNull { it.toQueueItem() },
                            upcomingTotal = lane.upcomingTotal,
                            pagedUpcoming = pagedUpcoming,
                        )
                    },
                revision = queueRevision,
            )
    }

    private fun Meta.toQueueItem(): QueueItem? {
        // Only the current track's metadata lacks an entryId, and it is never here.
        val entryId = entryId
        if (entryId == null) {
            logger.warning("queue entry $trackId has no entryId; dropping from projection")
            return null
        }
        return QueueItem(
            entryId = entryId,
            trackId = trackId,
            title = title,
            artist = artist,
            albumTitle = albumTitle,
            durationClock = durationClock,
            coverImage = coverImage,
        )
    }

    private fun BridgeQueueEntry.toEntry(): Meta =
        Meta(
            entryId = entryId,
            trackId = trackId,
            title = title,
            artist = artistNames,
            albumTitle = albumTitle,
            durationClock = durationClock,
            coverImage = coverImage,
        )

    // ── Media3 state ─────────────────────────────────────────────────────

    override fun getState(): State {
        val metas = orderedMetas(entries, currentMeta)
        // The notification and lock screen show only the current track's cover.
        val playlist =
            metas.map { meta ->
                val artwork = if (meta.trackId == playingTrackId) currentArtwork else null
                mediaItemData(meta, playingTrackId, positionModel.durationMs ?: C.TIME_UNSET, artwork)
            }

        val requestedIndex = playingTrackId?.let { id -> metas.indexOfFirst { it.trackId == id } }
        val currentIndex =
            when {
                playlist.isEmpty() -> {
                    C.INDEX_UNSET
                }

                requestedIndex == null -> {
                    C.INDEX_UNSET
                }

                requestedIndex >= 0 -> {
                    requestedIndex
                }

                else -> {
                    logger.warning(
                        "getState missing current track $playingTrackId in playlist ${metas.map { it.trackId }}",
                    )
                    C.INDEX_UNSET
                }
            }

        val playbackState = playbackStateFor(transport, playlistIsEmpty = playlist.isEmpty())

        return State
            .Builder()
            .setAvailableCommands(availableCommands(hasNext, hasPrevious))
            .setPlaybackState(playbackState)
            .setPlayWhenReady(playWhenReady, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setRepeatMode(media3RepeatMode)
            .setPlaylist(playlist)
            .setCurrentMediaItemIndex(currentIndex)
            .setContentPositionMs(
                PositionSupplier.getExtrapolating(
                    positionModel.effectivePositionMs.coerceAtLeast(0L),
                    if (playbackState == Player.STATE_READY && playWhenReady) 1.0f else 0.0f,
                ),
            ).build()
    }

    // ── Transport commands, forwarded to core ────────────────────────────────

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        if (playWhenReady) {
            appHandle.resume()
        } else {
            // A user pause must not be undone when audio focus comes back.
            systemHooks.disarmResumeOnFocusGain()
            appHandle.pause()
        }
        return Futures.immediateVoidFuture()
    }

    override fun handleStop(): ListenableFuture<*> {
        appHandle.stop()
        return Futures.immediateVoidFuture()
    }

    override fun handleSeek(
        mediaItemIndex: Int,
        positionMs: Long,
        seekCommand: Int,
    ): ListenableFuture<*> {
        when (seekCommand) {
            Player.COMMAND_SEEK_TO_NEXT,
            Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
            -> {
                runLoggedBridgeCommand(logger, "nextTrack") {
                    appHandle.nextTrack()
                }
            }

            Player.COMMAND_SEEK_TO_PREVIOUS,
            Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
            -> {
                runLoggedBridgeCommand(logger, "previousTrack") {
                    appHandle.previousTrack()
                }
            }

            Player.COMMAND_SEEK_TO_MEDIA_ITEM -> {
                // The current track's slot has no entry id, so seeking to it does nothing.
                val entryId = orderedMetas(entries, currentMeta).getOrNull(mediaItemIndex)?.entryId
                if (entryId != null) {
                    positionModel.clearPendingSeek()
                    appHandle.skipToEntry(entryId)
                } else {
                    logger.warning("handleSeek to media item $mediaItemIndex has no queue entry id")
                }
            }

            else -> {
                // COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM. Core seeks by ratio; the
                // requested position shows until core confirms it.
                val ratio = positionModel.beginInTrackSeek(playingTrackId, positionMs)
                if (ratio != null) {
                    appHandle.seekByRatio(ratio)
                    publish()
                } else {
                    logger.warning("handleSeek ignored: no duration for in-track seek")
                }
            }
        }
        return Futures.immediateVoidFuture()
    }

    /** Play a track chosen from the browse tree (Android Auto, a Bluetooth head unit). */
    override fun handleSetMediaItems(
        mediaItems: MutableList<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<*> {
        val item = mediaItems.getOrNull(startIndex) ?: mediaItems.firstOrNull()
        val mediaId = item?.mediaId
        when (val browseId = mediaId?.let { BrowseId.parse(it) }) {
            is BrowseId.Track -> appHandle.playRelease(browseId.releaseId, browseId.trackId, false)
            else -> logger.warning("handleSetMediaItems ignored non-track media id: $mediaId")
        }
        return Futures.immediateVoidFuture()
    }

    override fun handleSetRepeatMode(repeatMode: Int): ListenableFuture<*> {
        val mode =
            when (repeatMode) {
                Player.REPEAT_MODE_ONE -> BridgeRepeatMode.TRACK
                Player.REPEAT_MODE_ALL -> BridgeRepeatMode.CONTEXT
                else -> BridgeRepeatMode.OFF
            }
        appHandle.setRepeatMode(mode)
        return Futures.immediateVoidFuture()
    }

    override fun handleRelease(): ListenableFuture<*> {
        systemHooks.detach()
        return Futures.immediateVoidFuture()
    }

    fun closeSession() {
        systemHooks.detach()
        upcoming.close()
    }
}

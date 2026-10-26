package fm.bae.app.playback

import android.net.Uri
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
import kotlinx.coroutines.CoroutineName
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
import uniffi.bae_bridge.BridgeLoadingTrack
import uniffi.bae_bridge.BridgeNowPlayingTrack
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackPosition
import uniffi.bae_bridge.BridgePlaybackPositions
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackDisplay

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
    private val system: PlaybackSystem,
    private val scope: CoroutineScope,
    private val queueUpcomingSource: QueueUpcomingSource = QueueUpcomingSource(appHandle),
) : SimpleBasePlayer(applicationLooper),
    PlaybackEventSink {
    internal val systemHooks =
        PlaybackSystemHooks(
            system = system,
            appHandle = appHandle,
            scope = scope,
            isPlaying = { _isPlaying.value },
            hasCurrentTrack = { currentMeta != null },
        )

    /** One track of the Media3 playlist, from a queue entry or, for the current
     *  track, from the playback value. */
    internal data class Meta(
        /** Null for the current track's metadata, which is not a queue entry. */
        val entryId: String?,
        val trackId: String,
        val display: BridgeTrackDisplay,
    )

    /** The release or library the queue plays from, and its not-yet-played tracks. */
    internal data class ContextLane(
        val kind: BridgePlaybackSourceKind,
        val shuffled: Boolean,
        /** Only the first page of the [upcomingTotal] tracks. */
        val entries: List<BridgeQueueEntry>,
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
            artworkUri: Uri?,
        ): MediaItemData {
            val currentDurationMs = itemDurationMs(meta, playingTrackId, durationMs)
            val metadata =
                mediaMetadata(
                    meta,
                    if (currentDurationMs == C.TIME_UNSET) null else currentDurationMs,
                    artworkUri,
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
            artworkUri: Uri?,
        ): MediaMetadata =
            MediaMetadata
                .Builder()
                .setTitle(meta.display.title)
                .setArtist(meta.display.artistNames)
                .setAlbumTitle(meta.display.albumTitle)
                .setDurationMs(durationMs)
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .setIsPlayable(true)
                .setIsBrowsable(false)
                .setArtworkUri(artworkUri)
                .build()
    }

    /** What the Media3 playlist is built from: the up-next [entries], the
     *  [current] track, and its duration ([C.TIME_UNSET] when unknown). */
    private data class PlaylistSource(
        val entries: List<Meta>,
        val current: Meta?,
        val durationMs: Long,
    )

    /** The Media3 playlist built from [source]. */
    private class Playlist(
        val source: PlaylistSource,
        /** The tracks in play order; see [orderedMetas]. */
        val metas: List<Meta>,
        /** The current track's index in [metas], or [C.INDEX_UNSET]. */
        val currentIndex: Int,
        /**
         * A [State] holding only the playlist. [getState] builds each state on it,
         * so while the playlist is unchanged every state carries the same
         * [androidx.media3.common.Timeline], and Media3's comparison of the old and
         * new state finds the timelines equal by identity instead of comparing
         * every item.
         */
        val state: State,
    )

    internal enum class Transport { IDLE, BUFFERING, READY }

    private var transport: Transport = Transport.IDLE
    private var playWhenReady: Boolean = false

    /** The up-next tracks in play order ([manualEntries] then the context's), for
     *  the Media3 playlist. */
    private var entries: List<Meta> = emptyList()

    /** The queue's two sections, kept apart for the in-app queue. */
    private var manualEntries: List<BridgeQueueEntry> = emptyList()
    private var contextLane: ContextLane? = null

    /** Context tracks read past [ContextLane.entries], keyed by index in the whole tail; empty
     *  until a read for [queueRevision] arrives. */
    private var pagedUpcoming: Map<Int, BridgeQueueEntry> = emptyMap()

    /** The revision [manualEntries] and [contextLane] came from. */
    private var queueRevision: ULong = 0u

    /** The read of the context's tail past its first window; the queue shows its failure and retries it. */
    internal val upcoming =
        QueueUpcomingWindows(queueUpcomingSource, scope) { revision ->
            if (revision == queueRevision) {
                pagedUpcoming = upcomingEntries()
                publish()
            }
        }

    /** The current track's metadata, from the latest playback value. */
    private var currentMeta: Meta? = null

    private var sidePausePrompt: SidePausePrompt? = null

    /** Null when stopped. */
    private var playingTrackId: String? = null
    private var lastSeekRevision: ULong = 0u

    /** The latest library position core sent. Positions arrive on their own
     *  subscription, so one can come before the value that makes its track
     *  current; it is folded in once that track is. */
    private var latestPosition: BridgePlaybackPosition? = null
    private var hasNext: Boolean = false
    private var hasPrevious: Boolean = false
    private var media3RepeatMode: Int = Player.REPEAT_MODE_OFF

    private val positionModel = PlaybackPositionModel()

    /** The latest playlist [currentPlaylist] built. */
    private var playlist: Playlist = playlistFor(PlaylistSource(emptyList(), null, C.TIME_UNSET))

    private val _nowPlaying = MutableStateFlow<NowPlaying?>(null)
    val nowPlaying: StateFlow<NowPlaying?> = _nowPlaying.asStateFlow()

    private val _isPlaying = MutableStateFlow(false)
    val isPlaying: StateFlow<Boolean> = _isPlaying.asStateFlow()

    /** Failed reads of the values and the queue this player mirrors, each until its next value. */
    val readFailures = PlaybackReadFailures()

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
        readFailures.valuesArrived()
        val previousTrackId = playingTrackId
        val becameActive =
            when (val state = values.state) {
                BridgePlaybackValueState.Stopped -> {
                    onStopped()
                    false
                }

                is BridgePlaybackValueState.Loading -> {
                    onLoading(state.track)
                }

                is BridgePlaybackValueState.Playing -> {
                    activate(Transport.READY, state.track)
                }

                is BridgePlaybackValueState.Paused -> {
                    applyPaused(state)
                    false
                }
            }
        latestPosition
            ?.takeIf { playingTrackId != previousTrackId && it.trackId == playingTrackId }
            ?.let { fold(it, seeked = false) }
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
        if (becameActive) {
            systemHooks.onPlaybackActivated()
        }
    }

    fun applyPositions(positions: BridgePlaybackPositions) {
        val seeked = positions.seekRevision != lastSeekRevision
        lastSeekRevision = positions.seekRevision
        latestPosition = positions.library
        val position = positions.library ?: return
        when (fold(position, seeked)) {
            PositionUpdate.Applied -> {
                publish()
            }

            PositionUpdate.HeldByPendingSeek -> {
                Unit
            }

            PositionUpdate.StaleTrack -> {
                logger.debug("not showing position of ${position.trackId}; playing $playingTrackId")
            }
        }
    }

    /** Fold [position] into the position model, as a seek's confirmation when [seeked]. */
    private fun fold(
        position: BridgePlaybackPosition,
        seeked: Boolean,
    ): PositionUpdate {
        val trackId = position.trackId
        val durationMs = position.durationMs?.toLong()
        return if (seeked) {
            positionModel.onSeeked(playingTrackId, trackId, position.positionMs, durationMs, position.progress)
        } else {
            positionModel.onProgress(playingTrackId, trackId, position.positionMs, durationMs, position.progress)
        }
    }

    /** Enter loading for [track]; true when that made playback active. */
    private fun onLoading(track: BridgeLoadingTrack): Boolean {
        // Showing the loading track right away lets Media3 post the notification
        // and start the service while the app is still on screen; once the screen
        // locks, Android may refuse the start. Until core has prepared it, the
        // prior track stays on screen.
        val prepared =
            when (track) {
                is BridgeLoadingTrack.Unprepared -> null
                is BridgeLoadingTrack.Prepared -> track.track
            }
        return activate(Transport.BUFFERING, prepared)
    }

    /**
     * Enter play-when-ready for [transport]; a null [track] keeps the prior track.
     * True when that made playback active, which asks for the playback service.
     */
    private fun activate(
        transport: Transport,
        track: BridgeNowPlayingTrack?,
    ): Boolean {
        // A value also arrives when only the volume, mute, repeat mode or the
        // playing track's display changed, repeating the state already in force;
        // only a change of transport, of track, or into play-when-ready asks for
        // the playback service.
        val becameActive =
            !playWhenReady ||
                this.transport != transport ||
                (track != null && track.track.trackId != playingTrackId)
        this.transport = transport
        playWhenReady = true
        sidePausePrompt = null
        if (track != null) {
            makeCurrent(track)
        }
        return becameActive
    }

    /** Make [track] the current track. Its duration is null when unknown. */
    private fun makeCurrent(track: BridgeNowPlayingTrack) {
        val trackId = track.track.trackId
        positionModel.setActiveTrack(
            trackChanged = trackId != playingTrackId,
            durationMs = track.track.durationMs?.toLong(),
        )
        playingTrackId = trackId
        currentMeta =
            Meta(
                entryId = null,
                trackId = trackId,
                display = track.display,
            )
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
    }

    private fun onStopped() {
        transport = Transport.IDLE
        playWhenReady = false
        playingTrackId = null
        currentMeta = null
        sidePausePrompt = null
        positionModel.reset()
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
        val lane =
            context?.let {
                ContextLane(
                    kind = it.kind,
                    shuffled = it.shuffled,
                    entries = it.upcoming,
                    upcomingTotal = it.upcomingTotal.toInt(),
                )
            }
        if (revision < queueRevision) {
            logger.warning("dropping queue value at revision $revision; revision $queueRevision is already applied")
            return
        }
        readFailures.queueArrived()
        manualEntries = manual
        contextLane = lane
        if (revision > queueRevision) {
            queueRevision = revision
            pagedUpcoming = upcomingEntries()
        }
        entries = (manual + (lane?.entries ?: emptyList())).map { it.toMeta() }
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
    private fun upcomingEntries(): Map<Int, BridgeQueueEntry> = upcoming.entriesAt(queueRevision).toMap()

    /** Refresh the Media3 [State] and the in-app flows from the same fields. */
    private fun publish() {
        invalidateState()
        val meta = currentMeta
        _nowPlaying.value =
            meta?.let {
                NowPlaying(
                    it.trackId,
                    it.display.title,
                    it.display.artistNames,
                    it.display.coverImage,
                    sidePausePrompt,
                )
            }
        _isPlaying.value = transport == Transport.READY && playWhenReady
        _isLoading.value = transport == Transport.BUFFERING
        _position.value = positionModel.position(hasCurrentTrack = currentMeta != null)
        _queue.value =
            QueueProjection(
                manual = manualEntries,
                context =
                    contextLane?.let { lane ->
                        QueueContext(
                            kind = lane.kind,
                            shuffled = lane.shuffled,
                            upcoming = lane.entries,
                            upcomingTotal = lane.upcomingTotal,
                            pagedUpcoming = pagedUpcoming,
                        )
                    },
                revision = queueRevision,
            )
    }

    private fun BridgeQueueEntry.toMeta(): Meta = Meta(entryId = entryId, trackId = trackId, display = display)

    // ── Media3 state ─────────────────────────────────────────────────────

    override fun getState(): State {
        val playlist = currentPlaylist()
        val playbackState = playbackStateFor(transport, playlistIsEmpty = playlist.metas.isEmpty())

        return playlist.state
            .buildUpon()
            .setAvailableCommands(availableCommands(hasNext, hasPrevious))
            .setPlaybackState(playbackState)
            .setPlayWhenReady(playWhenReady, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setRepeatMode(media3RepeatMode)
            .setCurrentMediaItemIndex(playlist.currentIndex)
            .setContentPositionMs(
                PositionSupplier.getExtrapolating(
                    positionModel.effectivePositionMs.coerceAtLeast(0L),
                    if (playbackState == Player.STATE_READY && playWhenReady) 1.0f else 0.0f,
                ),
            ).build()
    }

    /** The playlist for the queue, the current track and its duration as they
     *  are now, built again only when one of them changed. */
    private fun currentPlaylist(): Playlist {
        val source = PlaylistSource(entries, currentMeta, positionModel.durationMs ?: C.TIME_UNSET)
        if (source != playlist.source) {
            playlist = playlistFor(source)
        }
        return playlist
    }

    private fun playlistFor(source: PlaylistSource): Playlist {
        val metas = orderedMetas(source.entries, source.current)
        val currentTrackId = source.current?.trackId
        // The notification and lock screen show only the current track's cover,
        // which Media3 reads through the artwork provider.
        val artworkUri =
            source.current
                ?.display
                ?.coverImage
                ?.let { ArtworkContentProvider.uriFor(system.context, it) }
        val items =
            metas.map { meta ->
                val itemArtworkUri = if (meta.trackId == currentTrackId) artworkUri else null
                mediaItemData(meta, currentTrackId, source.durationMs, itemArtworkUri)
            }

        val requestedIndex = currentTrackId?.let { id -> metas.indexOfFirst { it.trackId == id } }
        val currentIndex =
            when {
                metas.isEmpty() -> {
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
                        "playlist missing current track $currentTrackId in ${metas.map { it.trackId }}",
                    )
                    C.INDEX_UNSET
                }
            }

        return Playlist(source, metas, currentIndex, State.Builder().setPlaylist(items).build())
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
                // The session scope's handler reports this as a bug if it fails.
                scope.launch(CoroutineName("nextTrack")) { appHandle.nextTrack() }
            }

            Player.COMMAND_SEEK_TO_PREVIOUS,
            Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
            -> {
                scope.launch(CoroutineName("previousTrack")) { appHandle.previousTrack() }
            }

            Player.COMMAND_SEEK_TO_MEDIA_ITEM -> {
                // The current track's slot has no entry id, so seeking to it does nothing.
                val entryId = currentPlaylist().metas.getOrNull(mediaItemIndex)?.entryId
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

    /**
     * Play a track chosen from the browse tree (Android Auto, a Bluetooth head
     * unit), which the session callback resolved every request to; anything
     * else fails the request rather than reporting a play that didn't happen.
     */
    override fun handleSetMediaItems(
        mediaItems: MutableList<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<*> {
        // C.INDEX_UNSET is Media3 asking to start from the default item, the first.
        val index = if (startIndex == C.INDEX_UNSET) 0 else startIndex
        val mediaId = mediaItems.getOrNull(index)?.mediaId
        val track = mediaId?.let { BrowseId.parse(it) } as? BrowseId.Track
        if (track == null) {
            logger.error("asked to play $mediaId at $index of ${mediaItems.size}, which names no track")
            return Futures.immediateFailedFuture<Unit>(IllegalArgumentException("no track to play at $index"))
        }
        appHandle.playRelease(track.releaseId, track.trackId, false)
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

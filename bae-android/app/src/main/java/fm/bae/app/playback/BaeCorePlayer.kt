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
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgePlaybackValues
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.QueueUpcomingCallback

/** Now-playing snapshot the [fm.bae.app.ui.playback.NowPlayingBar] renders. */
private const val TAG = "bae.BaeCorePlayer"
private val logger = BaeLogger(TAG)

/**
 * Media3 [Player] that is a pure projection of bae-core's playback state.
 *
 * bae-core owns playback: it decodes audio (FFmpeg → AAudio) and publishes
 * retained values through `subscribePlaybackValues`. This player holds no audio;
 * it mirrors those values into a Media3 [State] so
 * the [androidx.media3.session.MediaSession] (and through it the notification,
 * lock screen, and in-app UI) reflects what core is doing.
 *
 * Single source of truth is core. Transport commands ([handleSetPlayWhenReady],
 * [handleSeekTo], …) forward to the bridge and make NO local state change; the
 * resulting retained value is what updates [State]. This avoids the optimistic
 * local mutation a normal [SimpleBasePlayer] does, which would fight core.
 *
 * Audio focus and becoming-noisy aren't handled by a custom [SimpleBasePlayer],
 * so this requests focus when playback starts and pauses core on focus loss /
 * unplug — the retained playback value reflects that pause back.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
class BaeCorePlayer(
    applicationLooper: Looper,
    private val appHandle: AppHandle,
    private val context: Context,
    private val scope: CoroutineScope,
    private val queuePageSource: QueuePageSource = QueuePageSource(appHandle),
    /**
     * Whether the app currently has a foreground (started) Activity. Android
     * forbids starting a service from the background, so [ensurePlaybackService]
     * only starts the service when this is true. Injected (not read from a global)
     * so the service-start decision is unit-testable.
     */
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

    /**
     * Display metadata for one track. Queue entries carry it from
     * `getQueueItems`; the current track's is overridden by the
     * playback-value payload (the authoritative source for
     * what's playing, and resilient to queue/playback event ordering).
     */
    internal data class Meta(
        /** The queue entry's per-instance id, or null for the current-track
         *  override (which is not a queue entry — see [orderedMetas]). */
        val entryId: String?,
        val trackId: String,
        val title: String,
        val artist: String,
        val albumTitle: String,
        /** The track length as a clock label's fields, or null when core reports
         *  none. Carried by queue entries; the Playing/Paused payload has no
         *  duration, so the current-track override leaves it null (the queue
         *  projection reads the duration off the queue entry, not this
         *  override). */
        val durationClock: BridgeDurationClock?,
        /** The cover whose bytes the now-playing artwork and the in-app rows
         *  fetch, or null when the track has no cover. */
        val coverImage: BridgeImageRef?,
    )

    /** The context lane (what the queue plays from): the [kind] it plays from
     *  (release vs library), whether it was ordered by shuffle, and its mapped
     *  not-yet-played tail. Held as one value so the whole lane is present or
     *  absent together — the bridge's nullable context maps straight onto it,
     *  with no discriminator re-derived from parallel fields. */
    internal data class ContextLane(
        val kind: BridgePlaybackSourceKind,
        val shuffled: Boolean,
        /** The initial window resolved eagerly by core — not the whole
         *  (library-scaled) tail. See [upcomingTotal]. */
        val entries: List<Meta>,
        val upcomingTotal: Int,
    )

    internal companion object {
        /**
         * The MediaSession playlist projection: the now-playing track followed
         * by the up-next queue. `entries` is core's up-next queue and EXCLUDES
         * the current track, so [current] is prepended (unless the queue somehow
         * already contains it); without this the session would render the first
         * up-next track as now-playing on the lock screen / notification / Auto.
         * Any queue entry matching the current id is substituted with [current]
         * so its display comes from the authoritative Playing/Paused payload.
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
         * Map the transport state to a Media3 [Player] playback state. An empty
         * playlist is forced to [Player.STATE_IDLE]: Media3's [SimpleBasePlayer]
         * asserts an empty playlist may only be STATE_IDLE or STATE_ENDED, and
         * during load (or a failed load) the transport is BUFFERING before the
         * first track's metadata hydrates the playlist. Without this guard that
         * empty-but-BUFFERING window crashes the app.
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
            // The cover is host-provided image bytes, not a file URI; Media3 decodes
            // the embedded artwork for the notification and lock screen. Only the
            // current track carries bytes (see getState), so queue items have none.
            artwork?.let {
                metadataBuilder.setArtworkData(it, MediaMetadata.PICTURE_TYPE_FRONT_COVER)
            }
            return metadataBuilder.build()
        }
    }

    internal enum class Transport { IDLE, BUFFERING, READY }

    private var transport: Transport = Transport.IDLE
    private var playWhenReady: Boolean = false

    /** The flat up-next playlist (manual lane then the context tail), hydrated
     *  from the latest queue value. This is the linear order the Media3
     *  session and skip-by-index need; the in-app two-section projection reads
     *  [manualEntries] / [contextLane] separately. */
    private var entries: List<Meta> = emptyList()

    /** The manual lane and the context lane, kept separate for the in-app
     *  two-section projection (`publish` builds `_queue` from these). The context
     *  lane is null when nothing plays from a release/library. */
    private var manualEntries: List<Meta> = emptyList()
    private var contextLane: ContextLane? = null

    /** Context-tail entries delivered past [ContextLane.entries]'s initial
     *  window, keyed by absolute index. Replaced when the queue revision moves. */
    private var pagedUpcoming: Map<Int, QueueItem> = emptyMap()

    /** The queue revision the current [manualEntries]/[contextLane] were built
     *  from. Stamped onto every [loadUpcomingRange] fetch so a reply computed
     *  under a since-superseded revision is dropped rather than merged. */
    private var queueRevision: ULong = 0u

    /** Live pages around the reported visible window. Moving the window evicts
     *  both subscriptions and their rows. */
    private var nextUpcomingSubscriptionIdentity = 0L
    private val upcomingSubscriptions =
        mutableMapOf<UpcomingPageKey, ActiveQueuePageSubscription>()

    /** Authoritative now-playing metadata from the latest Playing/Paused payload. */
    private var currentMeta: Meta? = null

    /** The cover image id the current artwork was (or is being) fetched for, and
     *  the fetched bytes once they arrive. The session embeds [currentArtwork] in
     *  the now-playing item's metadata (the notification/lock-screen art); only
     *  the current track's bytes are loaded, since that's the only art the system
     *  shows. Fetching is async, so getState renders without art until the bytes
     *  land and [refreshArtwork] republishes. */
    private var currentArtworkCover: BridgeImageRef? = null
    private var currentArtwork: ByteArray? = null

    private var sidePausePrompt: BridgeSidePausePrompt? = null

    /** Id of the now-playing track, or null when stopped. Mirrored into the
     *  [currentTrackId] StateFlow by [publish] for the in-app queue screen. */
    private var playingTrackId: String? = null
    private var lastSeekRevision: ULong = 0u
    private var hasNext: Boolean = false
    private var hasPrevious: Boolean = false
    private var media3RepeatMode: Int = Player.REPEAT_MODE_OFF

    /** Anchor/duration/progress/pending-seek transition state and the position and
     *  ratio derivations, kept as a pure unit the player feeds events and reads
     *  outputs from (the Media3 state and the position StateFlow). */
    private val positionModel = PlaybackPositionModel()

    // Compose-facing projection of the same state, for the in-app NowPlayingBar
    // (read directly off this player — no second MediaController transport).
    private val _nowPlaying = MutableStateFlow<NowPlaying?>(null)
    val nowPlaying: StateFlow<NowPlaying?> = _nowPlaying.asStateFlow()

    private val _isPlaying = MutableStateFlow(false)
    val isPlaying: StateFlow<Boolean> = _isPlaying.asStateFlow()

    // True while core is preparing or buffering a track (transport BUFFERING):
    // an initial load, or a seek to a position not yet downloaded. The in-app
    // bars swap the play/pause control for a spinner while this holds.
    private val _isLoading = MutableStateFlow(false)
    val isLoading: StateFlow<Boolean> = _isLoading.asStateFlow()

    private val _position = MutableStateFlow(PlaybackPosition(0.0, null, null))
    val position: StateFlow<PlaybackPosition> = _position.asStateFlow()

    // The two-lane queue projection the in-app QueueScreen renders as distinct
    // sections, derived in publish() from the same lanes the flat Media3 `entries`
    // are built from so the two can't drift. (The currently-playing track is not
    // in this projection — it rides `nowPlaying` — matching core's queue, which
    // holds only what's next.)
    private val _queue = MutableStateFlow(QueueProjection.EMPTY)
    val queue: StateFlow<QueueProjection> = _queue.asStateFlow()

    // Repeat mode for the in-app now-playing control. Driven by core's retained
    // playback value (which also sets Media3 `repeatMode`), so
    // the in-app button and the system controls reflect one source.
    private val _repeatMode = MutableStateFlow(BridgeRepeatMode.OFF)
    val repeatMode: StateFlow<BridgeRepeatMode> = _repeatMode.asStateFlow()

    // Output volume [0,1] and mute, for the expanded now-playing controls.
    // Driven by core's retained playback value, so the in-app slider/mute button
    // reflects core.
    // Independent of the Media3 State like _queue/_repeatMode — no publish().
    private val _volume = MutableStateFlow(1f)
    val volume: StateFlow<Float> = _volume.asStateFlow()

    private val _isMuted = MutableStateFlow(false)
    val isMuted: StateFlow<Boolean> = _isMuted.asStateFlow()

    // One-shot "+N added to queue" confirmations for the in-app root to surface
    // (a snackbar). A transient event, not projection state: replay = 0 so a
    // collector that resubscribes after the add — a recomposition, a screen
    // change — never re-shows a stale confirmation. Rapid adds drop the oldest
    // rather than backlog.
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

    /**
     * Toggle play/pause for a single tap (now-playing bar, expanded player,
     * track list). Reads the live play state so the call sites don't each
     * re-derive the same `if (isPlaying) pause() else play()` branch.
     */
    fun togglePlayPause() {
        if (_isPlaying.value) pause() else play()
    }

    // ── Event intake (called from UiEventAdapter on the application looper) ──

    fun applyValues(values: BridgePlaybackValues) {
        when (val state = values.state) {
            BridgePlaybackValueState.Stopped -> {
                onStopped()
            }

            is BridgePlaybackValueState.Loading -> {
                onLoading(state.trackId, state.track)
            }

            is BridgePlaybackValueState.Playing -> {
                applyPlaying(
                    state.trackId,
                    state.trackTitle,
                    state.artistNames,
                    state.albumTitle,
                    state.coverImage,
                    state.durationMs.toLong(),
                )
            }

            is BridgePlaybackValueState.Paused -> {
                applyPaused(
                    state.trackId,
                    state.trackTitle,
                    state.artistNames,
                    state.albumTitle,
                    state.coverImage,
                    state.durationMs.toLong(),
                    state.reason,
                )
            }
        }
        values.position?.let {
            if (values.seekRevision != lastSeekRevision) {
                onSeeked(it.trackId, it.positionMs.toLong(), it.durationMs.toLong(), it.progress)
            } else {
                onProgress(it.trackId, it.positionMs.toLong(), it.durationMs.toLong(), it.progress)
            }
        }
        lastSeekRevision = values.seekRevision
        onRepeatModeChanged(values.repeatMode)
        onVolumeChanged(values.volume)
        onMuteChanged(values.isMuted)
    }

    private fun onLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?,
    ) {
        // Once core resolves the target's metadata, project it as the current
        // track so the player has a non-empty timeline in STATE_BUFFERING — which
        // is what lets Media3 post the media notification and take the service
        // foreground while the track is still loading and the app is on screen,
        // instead of after the audio downloads (by when the screen may be locked
        // and a service start refused). The bare loading event (track == null)
        // passes no metadata, holding the prior track on screen with a spinner
        // until the swap — matching macOS. (Projecting a track that isn't in the
        // playlist yet would leave the current index unset.)
        val current =
            track?.let {
                Current(
                    meta(trackId, it.trackTitle, it.artistNames, it.albumTitle, it.coverImage),
                    it.durationMs.toLong(),
                )
            }
        activate(Transport.BUFFERING, current)
    }

    private fun applyPlaying(
        trackId: String,
        trackTitle: String,
        artistNames: String,
        albumTitle: String,
        coverImage: BridgeImageRef?,
        durationMs: Long,
    ) {
        activate(
            Transport.READY,
            Current(
                meta(trackId, trackTitle, artistNames, albumTitle, coverImage),
                durationMs,
            ),
        )
    }

    /** The resolved current track an activation swaps in: its display [meta] and
     *  core-reported raw [durationMs] (0 when unknown). The two always move
     *  together, so they're one payload — the bare loading event carries no
     *  [Current] at all and keeps the prior track. */
    private data class Current(
        val meta: Meta,
        val durationMs: Long,
    )

    /**
     * Enter an active (play-when-ready) state for [transport]. When core has
     * resolved the current track, [current] swaps in its metadata and duration;
     * the bare loading event passes null, keeping the prior track on screen so
     * the timeline never blinks empty mid-playback. Requests audio focus,
     * republishes the projection, and brings up the playback service.
     */
    private fun activate(
        transport: Transport,
        current: Current?,
    ) {
        this.transport = transport
        playWhenReady = true
        sidePausePrompt = null
        if (current != null) {
            positionModel.setActiveTrack(
                trackChanged = current.meta.trackId != playingTrackId,
                rawDurationMs = current.durationMs,
            )
            playingTrackId = current.meta.trackId
            currentMeta = current.meta
            refreshArtwork(current.meta.coverImage)
        }
        publish()
        systemHooks.onPlaybackActivated()
    }

    /**
     * Load the now-playing artwork bytes for [coverImage] and republish once they
     * land, so the session embeds them in the notification / lock-screen art. A
     * no-op when the cover hasn't changed (a pause on the same track keeps the
     * loaded bytes) — the reference pins the content version, so replacing a
     * release's cover does reload. A null reference, an absent image, or a fetch
     * failure clears the art (the loss is logged, not masked).
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
            // A later track change supersedes this fetch; only apply if the cover
            // is still current.
            if (currentArtworkCover == coverImage) {
                if (bytes == null) {
                    logger.warning("now-playing artwork bytes absent for ${coverImage.id}")
                }
                currentArtwork = bytes
                invalidateState()
            }
        }
    }

    private fun applyPaused(
        trackId: String,
        trackTitle: String,
        artistNames: String,
        albumTitle: String,
        coverImage: BridgeImageRef?,
        durationMs: Long,
        reason: BridgePlaybackPauseReason,
    ) {
        positionModel.setActiveTrack(
            trackChanged = trackId != playingTrackId,
            rawDurationMs = durationMs,
        )
        transport = Transport.READY
        playWhenReady = false
        playingTrackId = trackId
        currentMeta = meta(trackId, trackTitle, artistNames, albumTitle, coverImage)
        refreshArtwork(coverImage)
        sidePausePrompt =
            when (reason) {
                BridgePlaybackPauseReason.Manual -> null
                is BridgePlaybackPauseReason.SideEnded -> reason.prompt
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

    private fun meta(
        trackId: String,
        title: String,
        artist: String,
        albumTitle: String,
        coverImage: BridgeImageRef?,
    ): Meta =
        Meta(
            // The current-track override is not a queue entry, so it has no id.
            entryId = null,
            trackId = trackId,
            title = title,
            artist = artist,
            albumTitle = albumTitle,
            durationClock = null,
            coverImage = coverImage,
        )

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

    /** Republish when the position advanced; log a stale-track position; leave a
     *  projection held by a pending seek untouched. */
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

    private fun onRepeatModeChanged(mode: BridgeRepeatMode) {
        media3RepeatMode =
            when (mode) {
                BridgeRepeatMode.OFF -> Player.REPEAT_MODE_OFF
                BridgeRepeatMode.TRACK -> Player.REPEAT_MODE_ONE
                BridgeRepeatMode.CONTEXT -> Player.REPEAT_MODE_ALL
            }
        _repeatMode.value = mode
        publish()
    }

    private fun onVolumeChanged(volume: Float) {
        _volume.value = volume
    }

    private fun onMuteChanged(isMuted: Boolean) {
        _isMuted.value = isMuted
    }

    override fun onQueueItemsAdded(count: Int) {
        _queueItemsAdded.tryEmit(count)
    }

    /**
     * Apply the latest queue value. Core resolves each item's metadata
     * (including its cover image id) before delivery, so map both
     * lanes straight to entries. The flat [entries] (manual lane then the context
     * tail) drives the Media3 session and skip-by-index; the two lanes are also
     * kept apart for the in-app two-section projection.
     */
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
        val replacesPages = revision > queueRevision
        manualEntries = manualMetas
        contextLane = lane
        if (replacesPages) {
            upcomingSubscriptions.values.forEach { it.subscription.cancel() }
            upcomingSubscriptions.clear()
            pagedUpcoming = emptyMap()
        }
        queueRevision = revision
        entries = manualMetas + (lane?.entries ?: emptyList())
        this.hasNext = hasNext
        this.hasPrevious = hasPrevious
        publish()
    }

    /**
     * Subscribe to `[offset, offset + limit)` of the context's upcoming tail and
     * merge every delivered page into [pagedUpcoming]. A no-op if that range
     * already has a subscription. A newer queue revision cancels all prior page
     * subscriptions. Errors retain the last page and are logged because this is
     * background prefetch with no separate error UI.
     */
    suspend fun loadUpcomingRange(
        offset: Int,
        limit: Int,
    ) {
        val lane = contextLane ?: return
        val end = minOf(offset + limit, lane.upcomingTotal)
        if (offset >= end) return
        val key = UpcomingPageKey(offset until end, queueRevision)
        if (upcomingSubscriptions.containsKey(key)) return
        makeRoomForUpcomingPageNear(key.range)
        val identity = ++nextUpcomingSubscriptionIdentity
        upcomingSubscriptions[key] =
            ActiveQueuePageSubscription(identity, QueuePageSubscription {})
        val subscription =
            queuePageSource.subscribe(
                offset.toUInt(),
                (end - offset).toUInt(),
                object : QueueUpcomingCallback {
                    override fun onValue(value: uniffi.bae_bridge.BridgeQueueUpcomingPage) {
                        scope.launch {
                            if (upcomingSubscriptions[key]?.identity != identity) return@launch
                            if (value.revision != queueRevision) {
                                logger.warning(
                                    "dropping upcoming page for [$offset, $end): delivered for a since-superseded revision",
                                )
                                return@launch
                            }
                            val loaded =
                                value.entries
                                    .mapIndexedNotNull { i, entry ->
                                        entry.toEntry().toQueueItem()?.let { (offset + i) to it }
                                    }.toMap()
                            pagedUpcoming = pagedUpcoming + loaded
                            publish()
                        }
                    }

                    override fun onError(error: uniffi.bae_bridge.BridgeException) {
                        scope.launch {
                            if (upcomingSubscriptions[key]?.identity != identity) return@launch
                            logger.error(
                                "upcoming range [$offset, $end) subscription failed",
                                error,
                            )
                        }
                    }
                },
            )
        if (upcomingSubscriptions[key]?.identity == identity) {
            upcomingSubscriptions[key] = ActiveQueuePageSubscription(identity, subscription)
        } else {
            subscription.cancel()
        }
    }

    private fun makeRoomForUpcomingPageNear(visibleRange: IntRange) {
        while (upcomingSubscriptions.size >= MAX_UPCOMING_PAGE_SUBSCRIPTIONS) {
            val visibleMidpoint = visibleRange.first + visibleRange.count() / 2
            val key =
                upcomingSubscriptions.keys.maxBy { candidate ->
                    queuePageDistance(candidate.range, visibleMidpoint)
                }
            upcomingSubscriptions.remove(key)?.subscription?.cancel()
            val initialCount = contextLane?.entries?.size ?: 0
            pagedUpcoming = pagedUpcoming.filterKeys { it !in key.range || it < initialCount }
        }
    }

    /**
     * Push the rebuilt projection: refresh the Media3 [State] for the session,
     * and the Compose [StateFlow]s the in-app bar reads. Both render the same
     * single source — written here so they can't drift.
     */
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
        // Only the current-track override has a null entryId, and it never sits
        // in the up-next `entries` this maps over; a null here means a queue
        // entry arrived without its id.
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

    // ── State projection ─────────────────────────────────────────────────

    override fun getState(): State {
        // Now-playing first, then the up-next queue (see orderedMetas). entries
        // excludes the current track, so it must be prepended — otherwise the
        // session shows the first up-next track as now-playing.
        val metas = orderedMetas(entries, currentMeta)
        // Only the current track carries artwork bytes; the notification/lock
        // screen show that one cover, and fetching every queue item's would be
        // wasteful.
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
                    positionModel.effectivePositionMs,
                    if (playbackState == Player.STATE_READY && playWhenReady) 1.0f else 0.0f,
                ),
            ).build()
    }

    // ── Transport commands (forward to core; no local state change) ──────────

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        if (playWhenReady) {
            appHandle.resume()
        } else {
            // An explicit user pause must survive a later focus regain (e.g. a
            // dictation session ending), so disarm any transient-loss resume.
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
                // The Media3 playlist is the now-playing track followed by the
                // up-next entries (orderedMetas). Resolve the tapped index to its
                // queue-entry id; the current-track slot has none, so seeking to
                // it is a no-op.
                val entryId = orderedMetas(entries, currentMeta).getOrNull(mediaItemIndex)?.entryId
                if (entryId != null) {
                    positionModel.clearPendingSeek()
                    appHandle.skipToEntry(entryId)
                } else {
                    logger.warning("handleSeek to media item $mediaItemIndex has no queue entry id")
                }
            }

            else -> {
                // In-track seek: COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM. Core seeks by
                // ratio; the model derives it from the requested position and known
                // duration, and projects the dropped position until core confirms.
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
     * Play a library item chosen from the browse tree (Android Auto, a Bluetooth
     * head unit). The item carries a browse media id — not audio this player
     * decodes — so resolve the tapped id to a play-by-id command and forward it
     * to core, exactly as the in-app album detail plays a track. No local state
     * change: the resulting retained value updates [State], like every other
     * transport command here. Reached via COMMAND_SET_MEDIA_ITEM (a browse play
     * resolves to a single-item setMediaItem).
     */
    override fun handleSetMediaItems(
        mediaItems: MutableList<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): ListenableFuture<*> {
        val item = mediaItems.getOrNull(startIndex) ?: mediaItems.firstOrNull()
        val mediaId = item?.mediaId
        when (val browseId = mediaId?.let { BrowseId.parse(it) }) {
            is BrowseId.Track -> appHandle.playRelease(browseId.releaseId, browseId.index.toUInt(), false)
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

    fun detachSystemHooks() {
        systemHooks.detach()
    }

    fun cancelQueuePageSubscriptions() {
        upcomingSubscriptions.values.forEach { it.subscription.cancel() }
        upcomingSubscriptions.clear()
    }
}

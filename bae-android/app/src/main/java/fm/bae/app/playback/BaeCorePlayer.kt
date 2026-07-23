package fm.bae.app.playback

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
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
import kotlinx.coroutines.Job
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
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackPauseReason
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeSidePausePrompt
import uniffi.bae_bridge.BridgeUiEvent

/** Now-playing snapshot the [fm.bae.app.ui.playback.NowPlayingBar] renders. */
data class NowPlaying(
    val trackId: String,
    val title: String,
    val artist: String,
    /** The cover image id (a release id) the bar fetches bytes for, or null. */
    val coverImageId: String?,
    val sidePausePrompt: BridgeSidePausePrompt?,
)

/**
 * Live playback position for the seek bar. [progress] is the bridge's [0,1]
 * fraction for the slider; [positionMs] and [durationMs] are raw milliseconds,
 * which the seek bar turns into clock labels through core's projection. Null
 * means there is no label: nothing is playing, or the track's length is unknown.
 */
data class PlaybackPosition(
    val progress: Double,
    val positionMs: Long?,
    val durationMs: Long?,
)

/** One queue entry the [fm.bae.app.ui.playback.QueueScreen] renders. The UI projection
 *  of the player's internal queue metadata: [durationClock] is the track length
 *  as a clock label's fields (null when core reports none), which the row renders
 *  directly; [coverImageId] is the cover image id (a release id) the row fetches
 *  bytes for. [entryId] is the per-instance id the row keys on and that
 *  remove/reorder/skip target — unique even when the same track is queued
 *  twice. */
data class QueueItem(
    val entryId: String,
    val trackId: String,
    val title: String,
    val artist: String,
    val albumTitle: String,
    val durationClock: BridgeDurationClock?,
    val coverImageId: String?,
)

/** The context lane (the release being played from): the first page of its
 *  not-yet-played tail, the tail's full length, further pages fetched via
 *  [fm.bae.app.playback.BaeCorePlayer.loadUpcomingRange], plus whether it was
 *  ordered by shuffle (the UI shows a shuffle indicator when so). Rendered as a
 *  section distinct from the manual lane.
 *
 *  [upcoming] is only the initial window core resolved eagerly — the tail is
 *  library-scaled. [pagedUpcoming] holds indices fetched past that window,
 *  keyed by absolute index; [itemAt] reads either uniformly. */
data class QueueContext(
    val kind: BridgePlaybackSourceKind,
    val shuffled: Boolean,
    val upcoming: List<QueueItem>,
    val upcomingTotal: Int,
    val pagedUpcoming: Map<Int, QueueItem> = emptyMap(),
) {
    fun itemAt(index: Int): QueueItem? = upcoming.getOrNull(index) ?: pagedUpcoming[index]
}

/** The queue's two lanes the [fm.bae.app.ui.playback.QueueScreen] renders as distinct
 *  sections: the manual lane ("Up Next") and the [context] (or null when nothing
 *  plays from a release). Kept separate, not flattened. [revision] is the queue
 *  revision this projection was built from — a UI stamps its
 *  [BaeCorePlayer.loadUpcomingRange] fetches against it and drops a reply
 *  computed under a since-superseded revision. */
data class QueueProjection(
    val manual: List<QueueItem>,
    val context: QueueContext?,
    val revision: ULong = 0u,
) {
    companion object {
        val EMPTY = QueueProjection(manual = emptyList(), context = null)
    }
}

/**
 * The playback-state intake [BaeCorePlayer] exposes to [fm.bae.app.data.UiEventAdapter].
 * Event routing depends on this contract, not the concrete player, so playback
 * handling can be exercised against a fake sink.
 */
interface PlaybackEventSink {
    fun onLoading(
        trackId: String,
        track: BridgeLoadingTrackInfo?,
    )

    fun onPlaying(event: BridgeUiEvent.PlaybackPlaying)

    fun onPaused(event: BridgeUiEvent.PlaybackPaused)

    fun onStopped()

    fun onProgress(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    )

    fun onSeeked(
        trackId: String,
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    )

    fun onRepeatModeChanged(mode: BridgeRepeatMode)

    fun onQueueUpdated(
        manual: List<BridgeQueueEntry>,
        context: BridgePlaybackContext?,
        hasNext: Boolean,
        hasPrevious: Boolean,
        revision: ULong,
    )

    fun onVolumeChanged(volume: Float)

    fun onMuteChanged(isMuted: Boolean)

    fun onQueueItemsAdded(count: Int)
}

private const val TAG = "bae.BaeCorePlayer"
private val logger = BaeLogger(TAG)

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
private fun availableCommands(
    hasNext: Boolean,
    hasPrevious: Boolean,
): Player.Commands {
    val builder =
        Player.Commands
            .Builder()
            .add(Player.COMMAND_PLAY_PAUSE)
            .add(Player.COMMAND_STOP)
            .add(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_SEEK_TO_MEDIA_ITEM)
            // Lets a browse client (Android Auto / a head unit) play a tapped
            // library item: its play request resolves to a single-item
            // setMediaItem, which routes to handleSetMediaItems below. The
            // core-driven queue isn't editable through the raw media-item API, so
            // COMMAND_CHANGE_MEDIA_ITEMS (add/remove/move) stays unavailable.
            .add(Player.COMMAND_SET_MEDIA_ITEM)
            .add(Player.COMMAND_SET_REPEAT_MODE)
            .add(Player.COMMAND_GET_CURRENT_MEDIA_ITEM)
            .add(Player.COMMAND_GET_TIMELINE)
            .add(Player.COMMAND_GET_METADATA)

    listOf(
        hasNext to listOf(Player.COMMAND_SEEK_TO_NEXT, Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM),
        hasPrevious to listOf(Player.COMMAND_SEEK_TO_PREVIOUS, Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM),
    ).forEach { (enabled, commands) ->
        if (enabled) {
            commands.forEach { builder.add(it) }
        }
    }

    return builder.build()
}

internal class PlaybackSystemHooks(
    private val context: Context,
    private val appHandle: AppHandle,
    private val isAppForeground: () -> Boolean,
    private val isPlaying: () -> Boolean,
    private val hasCurrentTrack: () -> Boolean,
) {
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private val focusRequest: AudioFocusRequest =
        AudioFocusRequest
            .Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(
                AudioAttributes
                    .Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            ).setOnAudioFocusChangeListener(::onAudioFocusChange)
            .build()

    private var hasFocus: Boolean = false

    /**
     * Whether a transient focus loss paused *active* playback, so the matching
     * [AudioManager.AUDIOFOCUS_GAIN] should resume it. Left false when the loss
     * lands on an already-paused stream (e.g. dictation grabbing focus over a
     * track the user paused), so regaining focus never resumes a stream the user
     * stopped. Cleared on an explicit user pause.
     */
    private var resumeOnFocusGain: Boolean = false

    private val becomingNoisyReceiver =
        object : BroadcastReceiver() {
            override fun onReceive(
                c: Context?,
                intent: Intent?,
            ) {
                if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                    appHandle.pause()
                }
            }
        }
    private var noisyReceiverRegistered = false

    fun attach() {
        context.registerReceiver(
            becomingNoisyReceiver,
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
        )
        noisyReceiverRegistered = true
    }

    /**
     * Unregister the becoming-noisy receiver and abandon audio focus. Idempotent.
     * Called from [BaeCorePlayer.handleRelease] when Media3 releases the player,
     * and directly from session teardown BEFORE the [AppHandle] is closed: the
     * focus listener and noisy receiver call `appHandle.pause()/resume()`
     * spontaneously (no user action), so they must stop touching the handle
     * before it closes, or a stray system callback would hit a closed handle.
     */
    fun detach() {
        if (noisyReceiverRegistered) {
            context.unregisterReceiver(becomingNoisyReceiver)
            noisyReceiverRegistered = false
        }
        abandonFocus()
    }

    fun onPlaybackActivated() {
        requestFocus()
        ensurePlaybackService()
    }

    fun onPlaybackStopped() {
        abandonFocus()
    }

    fun disarmResumeOnFocusGain() {
        resumeOnFocusGain = false
    }

    /**
     * React to a system audio-focus change. Any loss pauses playback. A
     * *transient* loss (dictation, a navigation prompt, a notification chime)
     * arms [resumeOnFocusGain] only when it interrupted active playback, so the
     * matching [AudioManager.AUDIOFOCUS_GAIN] resumes a stream the loss itself
     * paused — but not one the user had already paused, which would otherwise
     * restart music the moment the interrupting app released focus. A permanent
     * loss never auto-resumes.
     */
    internal fun onAudioFocusChange(focusChange: Int) {
        when (focusChange) {
            AudioManager.AUDIOFOCUS_LOSS -> {
                resumeOnFocusGain = false
                appHandle.pause()
            }

            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT,
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK,
            -> {
                resumeOnFocusGain = isPlaying()
                appHandle.pause()
            }

            AudioManager.AUDIOFOCUS_GAIN -> {
                if (resumeOnFocusGain) {
                    resumeOnFocusGain = false
                    appHandle.resume()
                }
            }
        }
    }

    /**
     * Start the [PlaybackService] so the OS can keep this process — and core's
     * native audio thread — alive while the screen is off. Without a running
     * foreground service a backgrounded app is frozen under Doze and playback
     * dies.
     *
     * This only *starts* the service. Media3 owns the foreground promotion: it
     * posts the media notification and calls `startForeground` once the player is
     * engaged. Because [BaeCorePlayer.onLoading] projects the resolving track as
     * a current item in STATE_BUFFERING, that promotion happens while the track
     * is still loading and the app is on screen — before a slow download finishes
     * and the user locks the phone, which is when a foreground-service start
     * would be refused. Letting Media3 own the promotion (rather than calling
     * `startForegroundService` here) avoids both fighting it over the
     * notification and the "started a foreground service but never called
     * startForeground" crash when a track fails to load before the promotion
     * fires.
     *
     * Called after playback has already set play-when-ready and a
     * buffering/ready transport, so the gate left to check is whether there is a
     * current track and the app is on screen. Skip when there is no current track
     * (the bare loading event before metadata resolves — nothing to host yet) or
     * when the app is off screen: Android refuses a background service start, and
     * a play that begins off screen (auto-advance, a lock-screen control) is
     * already covered by the service started when this session began on screen.
     */
    private fun ensurePlaybackService() {
        val foreground = isAppForeground()
        val hasCurrentTrack = hasCurrentTrack()
        if (!hasCurrentTrack || !foreground) {
            logger.debug("Not starting playback service (currentTrack=$hasCurrentTrack, foreground=$foreground)")
            return
        }
        try {
            context.startService(Intent(context, PlaybackService::class.java))
        } catch (e: IllegalStateException) {
            // The app raced to the background between the foreground check and
            // this call, so Android refused the background service start. A
            // service started earlier in this playback session keeps playing;
            // there's nothing to recover.
            logger.warning("Could not start playback service from foreground", e)
        }
    }

    private fun requestFocus() {
        if (hasFocus) return
        val result = audioManager.requestAudioFocus(focusRequest)
        hasFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }

    private fun abandonFocus() {
        if (!hasFocus) return
        audioManager.abandonAudioFocusRequest(focusRequest)
        hasFocus = false
    }
}

/**
 * Media3 [Player] that is a pure projection of bae-core's playback state.
 *
 * bae-core owns playback: it decodes audio (FFmpeg → AAudio) and emits playback
 * state as [uniffi.bae_bridge.BridgeUiEvent] variants over `subscribeUiEvents`.
 * This player holds no audio; it mirrors those events into a Media3 [State] so
 * the [androidx.media3.session.MediaSession] (and through it the notification,
 * lock screen, and in-app UI) reflects what core is doing.
 *
 * Single source of truth is core. Transport commands ([handleSetPlayWhenReady],
 * [handleSeekTo], …) forward to the bridge and make NO local state change; the
 * resulting `Playback*` event is what updates [State]. This avoids the optimistic
 * local mutation a normal [SimpleBasePlayer] does, which would fight core.
 *
 * Audio focus and becoming-noisy aren't handled by a custom [SimpleBasePlayer],
 * so this requests focus when playback starts and pauses core on focus loss /
 * unplug — the pause reflects back as a `PlaybackPaused` event.
 */
@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
class BaeCorePlayer(
    applicationLooper: Looper,
    private val appHandle: AppHandle,
    private val context: Context,
    private val scope: CoroutineScope,
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
     * `PlaybackPlaying`/`PlaybackPaused` payload (the authoritative source for
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
        /** The cover image id (a release id) whose bytes the now-playing artwork
         *  and the in-app rows fetch, or null when the track has no cover. */
        val coverImageId: String?,
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
     *  from the latest `QueueUpdated` event. This is the linear order the Media3
     *  session and skip-by-index need; the in-app two-section projection reads
     *  [manualEntries] / [contextLane] separately. */
    private var entries: List<Meta> = emptyList()

    /** The manual lane and the context lane, kept separate for the in-app
     *  two-section projection (`publish` builds `_queue` from these). The context
     *  lane is null when nothing plays from a release/library. */
    private var manualEntries: List<Meta> = emptyList()
    private var contextLane: ContextLane? = null

    /** Context-tail entries fetched past [ContextLane.entries]'s initial window,
     *  keyed by absolute index. Reset wholesale on every `QueueUpdated` — that
     *  event is the invalidation signal for this ephemeral view cache, not
     *  durable state to reconcile incrementally. */
    private var pagedUpcoming: Map<Int, QueueItem> = emptyMap()

    /** The queue revision the current [manualEntries]/[contextLane] were built
     *  from. Stamped onto every [loadUpcomingRange] fetch so a reply computed
     *  under a since-superseded revision is dropped rather than merged. */
    private var queueRevision: ULong = 0u

    /** In-flight [loadUpcomingRange] fetches, keyed "offset:end:revision", so
     *  concurrent callers asking for the same range (every visible row in a
     *  batch window composing at once) coalesce onto one bridge call instead of
     *  each issuing a duplicate. */
    private val inFlightUpcomingLoads = mutableMapOf<String, Job>()

    /** Authoritative now-playing metadata from the latest Playing/Paused payload. */
    private var currentMeta: Meta? = null

    /** The cover image id the current artwork was (or is being) fetched for, and
     *  the fetched bytes once they arrive. The session embeds [currentArtwork] in
     *  the now-playing item's metadata (the notification/lock-screen art); only
     *  the current track's bytes are loaded, since that's the only art the system
     *  shows. Fetching is async, so getState renders without art until the bytes
     *  land and [refreshArtwork] republishes. */
    private var currentArtworkCoverId: String? = null
    private var currentArtwork: ByteArray? = null

    private var sidePausePrompt: BridgeSidePausePrompt? = null

    /** Id of the now-playing track, or null when stopped. Mirrored into the
     *  [currentTrackId] StateFlow by [publish] for the in-app queue screen. */
    private var playingTrackId: String? = null
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

    // Repeat mode for the in-app now-playing control. Driven by core's
    // RepeatModeChanged (the same event that sets the Media3 `repeatMode`), so
    // the in-app button and the system controls reflect one source.
    private val _repeatMode = MutableStateFlow(BridgeRepeatMode.OFF)
    val repeatMode: StateFlow<BridgeRepeatMode> = _repeatMode.asStateFlow()

    // Output volume [0,1] and mute, for the expanded now-playing controls.
    // Driven by core's VolumeChanged/MuteChanged (the same events the desktop
    // playback store consumes), so the in-app slider/mute button reflect core.
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

    override fun onLoading(
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
                    meta(trackId, it.trackTitle, it.artistNames, it.albumTitle, it.coverImageId),
                    it.durationMs.toLong(),
                )
            }
        activate(Transport.BUFFERING, current)
    }

    override fun onPlaying(event: BridgeUiEvent.PlaybackPlaying) {
        activate(
            Transport.READY,
            Current(
                meta(event.trackId, event.trackTitle, event.artistNames, event.albumTitle, event.coverImageId),
                event.durationMs.toLong(),
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
            refreshArtwork(current.meta.coverImageId)
        }
        publish()
        systemHooks.onPlaybackActivated()
    }

    /**
     * Load the now-playing artwork bytes for [coverImageId] (a release id) and
     * republish once they land, so the session embeds them in the notification /
     * lock-screen art. A no-op when the cover hasn't changed (a pause on the same
     * track keeps the loaded bytes). A null id, an absent image, or a fetch
     * failure clears the art (the loss is logged, not masked).
     */
    private fun refreshArtwork(coverImageId: String?) {
        if (coverImageId == currentArtworkCoverId) return
        currentArtworkCoverId = coverImageId
        currentArtwork = null
        if (coverImageId == null) {
            invalidateState()
            return
        }
        scope.launch {
            val bytes =
                try {
                    appHandle.fetchCoverImageBytes(coverImageId)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    logger.error("Failed to load now-playing artwork $coverImageId", e)
                    null
                }
            // A later track change supersedes this fetch; only apply if the cover
            // is still current.
            if (currentArtworkCoverId == coverImageId) {
                if (bytes == null) {
                    logger.warning("now-playing artwork bytes absent for $coverImageId")
                }
                currentArtwork = bytes
                invalidateState()
            }
        }
    }

    override fun onPaused(event: BridgeUiEvent.PlaybackPaused) {
        positionModel.setActiveTrack(
            trackChanged = event.trackId != playingTrackId,
            rawDurationMs = event.durationMs.toLong(),
        )
        transport = Transport.READY
        playWhenReady = false
        playingTrackId = event.trackId
        currentMeta = meta(event.trackId, event.trackTitle, event.artistNames, event.albumTitle, event.coverImageId)
        refreshArtwork(event.coverImageId)
        sidePausePrompt =
            when (val reason = event.reason) {
                BridgePlaybackPauseReason.Manual -> null
                is BridgePlaybackPauseReason.SideEnded -> reason.prompt
            }
        publish()
    }

    override fun onStopped() {
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
        coverImageId: String?,
    ): Meta =
        Meta(
            // The current-track override is not a queue entry, so it has no id.
            entryId = null,
            trackId = trackId,
            title = title,
            artist = artist,
            albumTitle = albumTitle,
            durationClock = null,
            coverImageId = coverImageId,
        )

    override fun onProgress(
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

    override fun onSeeked(
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

    override fun onRepeatModeChanged(mode: BridgeRepeatMode) {
        media3RepeatMode =
            when (mode) {
                BridgeRepeatMode.OFF -> Player.REPEAT_MODE_OFF
                BridgeRepeatMode.TRACK -> Player.REPEAT_MODE_ONE
                BridgeRepeatMode.CONTEXT -> Player.REPEAT_MODE_ALL
            }
        _repeatMode.value = mode
        publish()
    }

    override fun onVolumeChanged(volume: Float) {
        _volume.value = volume
    }

    override fun onMuteChanged(isMuted: Boolean) {
        _isMuted.value = isMuted
    }

    override fun onQueueItemsAdded(count: Int) {
        _queueItemsAdded.tryEmit(count)
    }

    /**
     * Hydrate the playlist from a `QueueUpdated` event. The core resolves each
     * item's metadata (including its cover image id) before emitting, so map both
     * lanes straight to entries. The flat [entries] (manual lane then the context
     * tail) drives the Media3 session and skip-by-index; the two lanes are also
     * kept apart for the in-app two-section projection.
     */
    override fun onQueueUpdated(
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
        manualEntries = manualMetas
        contextLane = lane
        // The event is the invalidation signal: every previously fetched page is
        // dropped, not reconciled against the new snapshot.
        pagedUpcoming = emptyMap()
        queueRevision = revision
        entries = manualMetas + (lane?.entries ?: emptyList())
        this.hasNext = hasNext
        this.hasPrevious = hasPrevious
        publish()
    }

    /**
     * Whether the context tail's absolute [index] is already loaded, either in
     * the initial window or [pagedUpcoming].
     */
    private fun upcomingItemLoaded(index: Int): Boolean {
        val lane = contextLane ?: return false
        return lane.entries.getOrNull(index)?.toQueueItem() != null || pagedUpcoming.containsKey(index)
    }

    /**
     * Fetch `[offset, offset + limit)` of the context's upcoming tail and merge
     * it into [pagedUpcoming]. A no-op if the range is already loaded. The reply
     * is applied only if its revision still matches [queueRevision]; a mismatch
     * means a `QueueUpdated` for a newer queue state arrived while the fetch was
     * in flight, and that event's wholesale reset already replaced the
     * projection, so the reply is dropped rather than merged. Fetch failures are
     * never swallowed silently: logged at warn with the failed range — a page
     * fetch is background prefetch, not a user-initiated action with its own
     * error-display path.
     */
    suspend fun loadUpcomingRange(
        offset: Int,
        limit: Int,
    ) {
        val lane = contextLane ?: return
        val end = minOf(offset + limit, lane.upcomingTotal)
        // Nothing to fetch: an empty/backwards range, or every row in it already
        // loaded. The `offset >= end` check short-circuits so the per-row scan
        // only runs on a non-empty range.
        if (offset >= end || (offset until end).all { upcomingItemLoaded(it) }) return

        // Coalesce concurrent requests for the same range+revision: the first
        // caller launches the fetch and removes the key once it settles; any
        // caller that finds an in-flight job just awaits it.
        val key = "$offset:$end:$queueRevision"
        val existing = inFlightUpcomingLoads[key]
        if (existing != null) {
            existing.join()
        } else {
            val job = scope.launch { fetchUpcomingRange(offset, end) }
            inFlightUpcomingLoads[key] = job
            job.join()
            inFlightUpcomingLoads.remove(key)
        }
    }

    private suspend fun fetchUpcomingRange(
        offset: Int,
        end: Int,
    ) {
        try {
            val page = appHandle.getQueueUpcomingPage(offset.toUInt(), (end - offset).toUInt())
            if (page.revision != queueRevision) {
                logger.warning(
                    "dropping upcoming page for [$offset, $end): fetched under a since-superseded revision",
                )
                return
            }
            val loaded =
                page.entries
                    .mapIndexedNotNull { i, entry -> entry.toEntry().toQueueItem()?.let { (offset + i) to it } }
                    .toMap()
            pagedUpcoming = pagedUpcoming + loaded
            publish()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("failed to load upcoming range [$offset, $end)", e)
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
                NowPlaying(it.trackId, it.title, it.artist, it.coverImageId, sidePausePrompt)
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
            coverImageId = coverImageId,
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
            coverImageId = coverImageId,
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
     * change: the resulting `Playback*` events update [State], like every other
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
}

package fm.bae.app.playback

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import fm.bae.app.BaeLogger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandle

private val systemHooksLogger = BaeLogger("bae.PlaybackSystemHooks")

/** What the device gives the open library's playback. */
class PlaybackSystem(
    val context: Context,
    /** The process's audio focus, whose changes playback follows. */
    val audioFocus: AudioFocus,
    /** Whether the app has a started Activity; Android refuses to start a service
     *  from the background, so [PlaybackSystemHooks] starts one only then. */
    val isAppForeground: () -> Boolean,
)

/**
 * What the system does to the open library's playback: the focus changes
 * [AudioFocus] reports pause and resume it, headphones coming out pause it, and
 * a start brings up the playback service. Core asks for focus before it starts
 * playback and gives it back once it stops; these hooks only follow what the
 * system does afterwards.
 */
internal class PlaybackSystemHooks(
    private val system: PlaybackSystem,
    private val appHandle: AppHandle,
    private val scope: CoroutineScope,
    private val isPlaying: () -> Boolean,
    private val hasCurrentTrack: () -> Boolean,
) {
    private val context = system.context

    /**
     * Whether a transient focus loss paused *active* playback, so the matching
     * [AudioManager.AUDIOFOCUS_GAIN] should resume it. Left false when the loss
     * lands on an already-paused stream (e.g. dictation grabbing focus over a
     * track the user paused), so regaining focus never resumes a stream the user
     * stopped. Cleared on an explicit user pause.
     */
    private var resumeOnFocusGain: Boolean = false

    /** Following [AudioFocus.changes] while attached. */
    private var focusChanges: Job? = null

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
        focusChanges = scope.launch { system.audioFocus.changes.collect(::onAudioFocusChange) }
    }

    /**
     * Unregister the becoming-noisy receiver and stop following focus changes.
     * Idempotent. Called from [BaeCorePlayer.handleRelease] when Media3 releases
     * the player, and directly from session teardown BEFORE the [AppHandle] is
     * closed: focus changes and the noisy receiver call
     * `appHandle.pause()/resume()` spontaneously (no user action), so they must
     * stop touching the handle before it closes, or a stray system callback
     * would hit a closed handle.
     */
    fun detach() {
        if (noisyReceiverRegistered) {
            context.unregisterReceiver(becomingNoisyReceiver)
            noisyReceiverRegistered = false
        }
        focusChanges?.cancel()
        focusChanges = null
    }

    fun onPlaybackActivated() {
        ensurePlaybackService()
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
        val foreground = system.isAppForeground()
        val hasCurrentTrack = hasCurrentTrack()
        if (!hasCurrentTrack || !foreground) {
            systemHooksLogger.debug(
                "Not starting playback service " +
                    "(currentTrack=$hasCurrentTrack, foreground=$foreground)",
            )
            return
        }
        try {
            context.startService(Intent(context, PlaybackService::class.java))
        } catch (e: IllegalStateException) {
            // The app raced to the background between the foreground check and
            // this call, so Android refused the background service start. A
            // service started earlier in this playback session keeps playing;
            // there's nothing to recover.
            systemHooksLogger.warning("Could not start playback service from foreground", e)
        }
    }
}

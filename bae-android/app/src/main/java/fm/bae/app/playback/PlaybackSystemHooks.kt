package fm.bae.app.playback

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import fm.bae.app.BaeLogger
import uniffi.bae_bridge.AppHandle

private val systemHooksLogger = BaeLogger("bae.PlaybackSystemHooks")

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

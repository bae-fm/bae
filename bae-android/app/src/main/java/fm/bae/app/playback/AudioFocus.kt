package fm.bae.app.playback

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import uniffi.bae_bridge.BridgeAudioFocus
import uniffi.bae_bridge.BridgeAudioFocusAnswer

/**
 * Android's audio focus for bae's playback, one per process: the system grants
 * focus to an app, not to a library. Core asks for it before every start of
 * playback and gives it back once playback stops, so a start the system
 * refuses, while a call holds focus, never plays. What the system does to the
 * focus afterwards (a call, a navigation prompt, another music app) goes out
 * on [changes] for the open library's playback to follow.
 */
class AudioFocus(
    context: Context,
) : BridgeAudioFocus {
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private val _changes =
        MutableSharedFlow<Int>(
            replay = 0,
            extraBufferCapacity = 8,
            onBufferOverflow = BufferOverflow.DROP_OLDEST,
        )

    /** Each `AudioManager.AUDIOFOCUS_*` change the system sends, on the main thread. */
    val changes: SharedFlow<Int> = _changes.asSharedFlow()

    private val focusRequest: AudioFocusRequest =
        AudioFocusRequest
            .Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(
                AudioAttributes
                    .Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            ).setOnAudioFocusChangeListener(::onChange, Handler(Looper.getMainLooper()))
            .build()

    /**
     * Ask for focus before playback starts, on core's playback thread. Asking
     * again while holding it is granted again, so a focus the system took back
     * is always asked for afresh.
     */
    override fun request(): BridgeAudioFocusAnswer {
        val result = audioManager.requestAudioFocus(focusRequest)
        return if (result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
            BridgeAudioFocusAnswer.Granted
        } else {
            BridgeAudioFocusAnswer.Refused("request result $result, audio mode ${audioManager.mode}")
        }
    }

    /** Give focus back once playback stops, on core's playback thread. */
    override fun release() {
        audioManager.abandonAudioFocusRequest(focusRequest)
    }

    internal fun onChange(focusChange: Int) {
        _changes.tryEmit(focusChange)
    }
}

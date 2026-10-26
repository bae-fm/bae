package fm.bae.app.playback

import android.content.Context
import android.media.AudioManager
import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlaybackPauseReason

/**
 * Audio-focus pause/resume policy. A transient focus loss (a dictation session,
 * a navigation prompt) pauses playback, and the matching regain resumes it — but
 * only when the loss interrupted *active* playback. A loss that arrives over a
 * stream the user already paused must not resume on regain, or toggling
 * dictation over a paused track would restart music the user had stopped.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AudioFocusResumeTest {
    private fun player(
        context: Context,
        handle: FakeAppHandle,
        audioFocus: AudioFocus = AudioFocus(context),
    ): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            system = PlaybackSystem(context, audioFocus, { false }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )

    private fun BaeCorePlayer.startPlaying() =
        applyPlaybackState(
            playingState(
                trackId = "t1",
                durationMs = 200_000uL,
            ),
        )

    private fun BaeCorePlayer.reportPaused() =
        applyPlaybackState(
            pausedState(
                trackId = "t1",
                durationMs = 200_000uL,
                reason = BridgePlaybackPauseReason.Manual,
            ),
        )

    /** A change the system sends to the process's focus reaches the open library's playback. */
    @Test
    fun aFocusChangeTheSystemSendsPausesPlayback() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle()
        val audioFocus = AudioFocus(context)
        val player = player(context, handle, audioFocus)

        player.startPlaying()
        audioFocus.onChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.pauseCount)
    }

    @Test
    fun transientLossWhileUserPausedDoesNotResumeOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle()
        val player = player(context, handle)

        player.startPlaying()
        player.reportPaused() // the user paused
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        // Dictation grabs focus, then releases it.
        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn, handle.resumeCount)
    }

    @Test
    fun transientLossWhilePlayingResumesOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle()
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn + 1, handle.resumeCount)
    }

    @Test
    fun permanentLossDoesNotResumeOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle()
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS)
        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn, handle.resumeCount)
    }

    @Test
    fun userPauseDuringTransientLossDisarmsResume() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle()
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()

        // A transient loss while playing arms resume...
        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        // ...but the user pauses during the interruption (lock screen / notification),
        // which must disarm it.
        player.pause()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeRegain = handle.resumeCount

        player.systemHooks.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeRegain, handle.resumeCount)
    }
}

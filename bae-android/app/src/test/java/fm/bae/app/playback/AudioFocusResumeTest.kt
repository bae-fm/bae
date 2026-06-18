package fm.bae.app.playback

import android.content.Context
import android.media.AudioManager
import android.os.Looper
import fm.bae.app.data.Library
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
    ): BaeCorePlayer =
        BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = handle,
            library = Library(handle),
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )

    private fun BaeCorePlayer.startPlaying() = onPlaying("t1", "Track Title", "Artist Name", "Album Title", null, 200_000L)

    private fun BaeCorePlayer.reportPaused() = onPaused("t1", "Track Title", "Artist Name", "Album Title", null, 200_000L)

    @Test
    fun transientLossWhileUserPausedDoesNotResumeOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle(emptyMap())
        val player = player(context, handle)

        player.startPlaying()
        player.reportPaused() // the user paused
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        // Dictation grabs focus, then releases it.
        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn, handle.resumeCount)
    }

    @Test
    fun transientLossWhilePlayingResumesOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle(emptyMap())
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn + 1, handle.resumeCount)
    }

    @Test
    fun permanentLossDoesNotResumeOnRegain() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle(emptyMap())
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeFocusChurn = handle.resumeCount

        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS)
        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeFocusChurn, handle.resumeCount)
    }

    @Test
    fun userPauseDuringTransientLossDisarmsResume() {
        val context = RuntimeEnvironment.getApplication()
        val handle = FakeAppHandle(emptyMap())
        val player = player(context, handle)

        player.startPlaying()
        shadowOf(Looper.getMainLooper()).idle()

        // A transient loss while playing arms resume...
        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT)
        // ...but the user pauses during the interruption (lock screen / notification),
        // which must disarm it.
        player.pause()
        shadowOf(Looper.getMainLooper()).idle()
        val resumesBeforeRegain = handle.resumeCount

        player.onAudioFocusChange(AudioManager.AUDIOFOCUS_GAIN)

        assertEquals(resumesBeforeRegain, handle.resumeCount)
    }
}

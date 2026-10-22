package fm.bae.app.playback

import android.content.Context
import android.media.AudioManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAudioFocusAnswer

/**
 * The gate core asks before every start of playback: Android's answer to a
 * focus request goes back to core as is, and stopping gives the focus back.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AudioFocusTest {
    private val context = RuntimeEnvironment.getApplication()
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    @Test
    fun aGrantedRequestLetsPlaybackStart() {
        shadowOf(audioManager).setNextFocusRequestResponse(AudioManager.AUDIOFOCUS_REQUEST_GRANTED)

        assertEquals(BridgeAudioFocusAnswer.Granted, AudioFocus(context).request())
    }

    /** While a call holds focus the request fails, and core is told why. */
    @Test
    fun aRefusedRequestKeepsPlaybackFromStartingAndSaysWhy() {
        shadowOf(audioManager).setNextFocusRequestResponse(AudioManager.AUDIOFOCUS_REQUEST_FAILED)

        val answer = AudioFocus(context).request()

        assertTrue("refused, got $answer", answer is BridgeAudioFocusAnswer.Refused)
        assertTrue(
            (answer as BridgeAudioFocusAnswer.Refused).reason.contains("audio mode"),
        )
    }

    /** A focus the system took back is asked for again, not assumed still held. */
    @Test
    fun everyStartAsksTheSystem() {
        val audioFocus = AudioFocus(context)
        audioFocus.request()
        audioFocus.onChange(AudioManager.AUDIOFOCUS_LOSS)
        shadowOf(audioManager).setNextFocusRequestResponse(AudioManager.AUDIOFOCUS_REQUEST_FAILED)

        assertTrue(audioFocus.request() is BridgeAudioFocusAnswer.Refused)
    }

    @Test
    fun releasingGivesTheFocusBack() {
        val audioFocus = AudioFocus(context)
        audioFocus.request()

        audioFocus.release()

        assertNotNull(shadowOf(audioManager).lastAbandonedAudioFocusRequest)
    }
}

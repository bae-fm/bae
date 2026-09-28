package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioDifference
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BridgeFormatTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val stereoKey: (Long) -> String? = { "core.audio.channels.stereo" }

    @Test
    fun cueDescriptorUsesTheSharedLocalizedAudioFacts() {
        val summary =
            BridgeSourceAudioSummary.Uniform(
                BridgeSourceAudioDescriptor(
                    layout = BridgeSourceAudioLayout.CUE,
                    format =
                        BridgeAudioFormat(
                            codec = "FLAC",
                            sampleRateHz = 44_100,
                            bitsPerSample = 16,
                            bitrateKbps = null,
                            channels = 2,
                        ),
                ),
            )

        assertEquals(
            "CUE · FLAC · 44.1 kHz · 16-bit · stereo",
            summary.text(context, stereoKey),
        )
    }

    /** A mixed summary names what differs, each fact's values listed the locale's way. */
    @Test
    fun mixedSummaryNamesWhatDiffers() {
        val summary =
            BridgeSourceAudioSummary.Mixed(
                listOf(
                    BridgeSourceAudioDifference.Layout(
                        listOf(BridgeSourceAudioLayout.CUE, BridgeSourceAudioLayout.FILE),
                    ),
                    BridgeSourceAudioDifference.BitDepth(listOf(16L, 24L)),
                    BridgeSourceAudioDifference.Channels(listOf(2L, 1L)),
                ),
            )
        val channelsKey: (Long) -> String? = {
            if (it == 1L) "core.audio.channels.mono" else "core.audio.channels.stereo"
        }

        assertEquals(
            "CUE and track files · 16-bit and 24-bit · stereo and mono",
            summary.text(context, channelsKey),
        )
    }
}

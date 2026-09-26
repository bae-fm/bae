package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAudioFormat
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeFile
import uniffi.bae_bridge.BridgeSourceAudioDescriptor
import uniffi.bae_bridge.BridgeSourceAudioLayout
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeTermLabel

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BridgeReleaseFormatTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val stereoKey: (Long) -> String? = { "core.audio.channels.stereo" }

    /** What the pressing is sits after the year, its details after the catalog number. */
    @Test
    fun pressingFactsTakeTheirPlacesInTheLine() {
        val release = BridgeFixtures.release(id = "release-1", albumId = "album-1")
        assertEquals(
            "Japan · CD · Promo",
            release.compactMetadataText(
                context,
                stereoKey,
                {
                    listOf(
                        BridgeFactTerm.Country("JP"),
                        BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD")),
                    )
                },
                { listOf(BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.status.promotion"))) },
            ),
        )
    }

    @Test
    fun releaseMetadataUsesTheAllFilesSummaryInsteadOfTheFirstFile() {
        val flac =
            BridgeAudioFormat(
                codec = "FLAC",
                sampleRateHz = 44_100,
                bitsPerSample = 16,
                bitrateKbps = null,
                channels = 2,
            )
        val mp3 =
            BridgeAudioFormat(
                codec = "MP3",
                sampleRateHz = 48_000,
                bitsPerSample = null,
                bitrateKbps = 320,
                channels = 2,
            )
        val release =
            BridgeFixtures.release(
                id = "release-1",
                albumId = "album-1",
                files =
                    listOf(
                        BridgeFile(
                            id = "file-1",
                            originalFilename = "01.flac",
                            fileSize = 1_000,
                            contentType = "audio/flac",
                            isImage = false,
                            audioFormat = flac,
                        ),
                    ),
                sourceAudio =
                    BridgeSourceAudioSummary.Mixed(
                        listOf(
                            BridgeSourceAudioDescriptor(BridgeSourceAudioLayout.FILE, flac),
                            BridgeSourceAudioDescriptor(BridgeSourceAudioLayout.FILE, mp3),
                        ),
                    ),
            )

        assertEquals(
            "Various · FLAC · 44.1 kHz · 16-bit · stereo · " +
                "MP3 · 320 kbps · 48 kHz · stereo",
            release.compactMetadataText(context, stereoKey, { emptyList() }, { emptyList() }),
        )
    }
}

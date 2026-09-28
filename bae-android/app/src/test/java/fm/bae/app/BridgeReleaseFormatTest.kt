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
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeSourceAudioDifference
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeTermLabel

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BridgeReleaseFormatTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val stereoKey: (Long) -> String? = { "core.audio.channels.stereo" }

    /** What the pressing is sits after the year, its details after it. */
    @Test
    fun pressingFactsTakeTheirPlacesInTheLine() {
        val release =
            BridgeFixtures
                .release(id = "release-1", albumId = "album-1")
                .copy(
                    pressingSummary =
                        listOf(
                            BridgeFactTerm.Country("JP"),
                            BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD")),
                        ),
                    pressingDetails =
                        listOf(BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.status.promotion"))),
                )
        assertEquals("Japan · CD · Promo", release.pressingLineText(context, stereoKey))
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
                            BridgeSourceAudioDifference.Codec(listOf("FLAC", "MP3")),
                            BridgeSourceAudioDifference.SampleRate(listOf(44_100L, 48_000L)),
                        ),
                    ),
            )

        // A mixed release names what differs.
        assertEquals(
            "FLAC and MP3 · 44.1 kHz and 48 kHz",
            release.pressingLineText(context, stereoKey),
        )
    }

    /** Each label line shows its names, then the numbers they share, once. */
    @Test
    fun labelLinesShowTheirNamesThenTheirNumbers() {
        val lines =
            listOf(
                BridgeLabelLine(names = listOf("Label One", "Label Two"), catalogNumbers = listOf("AB-100")),
                BridgeLabelLine(names = listOf("Label Three"), catalogNumbers = listOf("CD-200", "EF-300")),
            )
        assertEquals(
            "Label One, Label Two · AB-100 · Label Three · CD-200, EF-300",
            labelsLineText(context, lines),
        )
    }
}

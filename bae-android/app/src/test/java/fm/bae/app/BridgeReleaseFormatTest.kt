package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeSourceAudioDifference
import uniffi.bae_bridge.BridgeSourceAudioSummary
import uniffi.bae_bridge.BridgeTermLabel

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class BridgeReleaseFormatTest {
    private val context: Context = RuntimeEnvironment.getApplication()
    private val stereoKey: (Long) -> String? = { "core.audio.channels.stereo" }

    private fun release() = BridgeFixtures.release(id = "release-1", albumId = "album-1")

    /** The edition line holds the label and catalog number, then the country. */
    @Test
    fun theEditionLineNamesTheLabelBeforeTheCountry() {
        val release =
            release().copy(
                labels = listOf(BridgeLabelLine(names = listOf("Harbor Records"), catalogNumbers = listOf("HR-1042"))),
                areaTerms = listOf(BridgeFactTerm.Country("JP")),
            )

        assertEquals("Harbor Records · HR-1042 · Japan", release.editionLineText(context))
    }

    /** Core hands over the release's year only when it differs from the album's: the line starts with it then. */
    @Test
    fun theEditionLineNamesTheYearOnlyWhenCoreHandsOneOver() {
        val base =
            release().copy(
                labels = listOf(BridgeLabelLine(names = listOf("Harbor Records"), catalogNumbers = emptyList())),
            )

        assertEquals("2001 · Harbor Records", base.copy(distinctYear = 2001).editionLineText(context))
        assertEquals("Harbor Records", base.copy(distinctYear = null).editionLineText(context))
    }

    /** The copy line holds the media, then the details after them. */
    @Test
    fun theCopyLineNamesTheMediaThenTheDetails() {
        val release =
            release().copy(
                mediaTerms = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD"))),
                pressingDetails = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Localized("core.pressing.status.promotion"))),
            )

        assertEquals("CD · Promo", release.copyLineText(context))
    }

    @Test
    fun aMixedReleaseAudioLineNamesWhatDiffers() {
        val release =
            BridgeFixtures.release(
                id = "release-1",
                albumId = "album-1",
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
            release.audioLineText(context, stereoKey),
        )
    }

    @Test
    fun aReleaseWithNoReadAudioHasNoAudioLine() {
        assertEquals("", release().audioLineText(context, stereoKey))
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

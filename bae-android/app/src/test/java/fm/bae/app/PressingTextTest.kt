package fm.bae.app

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeTermLabel

/**
 * The parts of a pressing's line, worded: a country from its code, a catalog
 * word through the string resources, a printed term as printed, a count
 * through its pattern. Which parts a line has is core's, tested there.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PressingTextTest {
    private val context: Context = RuntimeEnvironment.getApplication()

    private fun localized(key: String) = BridgeFactTerm.Worded(BridgeTermLabel.Localized(key))

    @Test
    fun aSummaryNamesTheCountryAndTheMedia() {
        assertEquals(
            "Japan · CD",
            factLine(
                context,
                listOf(BridgeFactTerm.Country("JP"), BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD"))),
            ),
        )
        assertEquals(
            "UK & Europe · 2×Vinyl",
            factLine(
                context,
                listOf(
                    localized("core.pressing.region.uk_and_europe"),
                    BridgeFactTerm.Counted(2u, BridgeTermLabel.Localized("core.pressing.medium.vinyl")),
                ),
            ),
        )
    }

    @Test
    fun detailsAreWordedThroughTheCatalogOrAsPrinted() {
        assertEquals(
            "Promo · Reissue · FLAC",
            factLine(
                context,
                listOf(
                    localized("core.pressing.status.promotion"),
                    localized("core.pressing.discogs.reissue"),
                    BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("FLAC")),
                ),
            ),
        )
        assertEquals("", factLine(context, emptyList()))
    }

    @Test
    fun anUnnamedReleaseIsNumbered() {
        assertEquals("Release 3", BridgeReleaseName.Numbered(3).text(context))
        assertEquals("Deluxe", BridgeReleaseName.Named("Deluxe").text(context))
    }

    /** A release with no name reads as its year and the media core worded. */
    @Test
    fun aDescribedReleaseReadsAsItsYearAndMedia() {
        assertEquals(
            "2016 2×CD",
            BridgeReleaseName.Described(2016, listOf(BridgeFactTerm.Counted(2u, BridgeTermLabel.Verbatim("CD")))).text(context),
        )
    }
}

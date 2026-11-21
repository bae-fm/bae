package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.albumdetail.AndroidRecordLinks
import org.junit.Test
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeReleaseRecord

/** The component shots of `AndroidRecordLinks`, one for each row of its preview. */
class AndroidRecordLinksShots : BoardShot() {
    @Test
    fun androidComponentRecordLinksOneCatalog() =
        component("android.component.record-links.one-catalog") { Links(BridgeCatalog.MUSIC_BRAINZ) }

    @Test
    fun androidComponentRecordLinksTwoCatalogs() =
        component("android.component.record-links.two-catalogs") {
            Links(BridgeCatalog.MUSIC_BRAINZ, BridgeCatalog.DISCOGS)
        }

    @Test
    fun androidComponentRecordLinksWrapping() =
        component("android.component.record-links.wrapping") {
            Links(
                BridgeCatalog.MUSIC_BRAINZ,
                BridgeCatalog.DISCOGS,
                BridgeCatalog.BANDCAMP,
                BridgeCatalog.WIKIDATA,
                BridgeCatalog.APPLE_MUSIC,
            )
        }
}

@Composable
private fun Links(vararg catalogs: BridgeCatalog) =
    AndroidRecordLinks(catalogs.map { BridgeReleaseRecord(it, "https://example.com") }) { catalog -> CATALOG_NAMES.getValue(catalog) }

private val CATALOG_NAMES =
    mapOf(
        BridgeCatalog.MUSIC_BRAINZ to "MusicBrainz",
        BridgeCatalog.DISCOGS to "Discogs",
        BridgeCatalog.BANDCAMP to "Bandcamp",
        BridgeCatalog.WIKIDATA to "Harbor Records catalog",
        BridgeCatalog.APPLE_MUSIC to "Library of releases",
    )

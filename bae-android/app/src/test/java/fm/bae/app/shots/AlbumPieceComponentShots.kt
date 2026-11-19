package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.creditLineNamed
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumCredits
import fm.bae.app.ui.albumdetail.AndroidRecordLinks
import fm.bae.app.ui.albumdetail.AndroidReleasePicker
import fm.bae.app.ui.library.AndroidArtistOtherNames
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeReleaseRecord
import uniffi.bae_bridge.BridgeTermLabel

/** The component shots of the album page's pieces: record links, release picker, credits, and the artist page's other names. */
class AlbumPieceComponentShots : BoardShot() {
    @Test
    fun androidComponentRecordLinksOneCatalog() =
        component("android.component.record-links.one-catalog", wide = true) { Links(BridgeCatalog.MUSIC_BRAINZ) }

    @Test
    fun androidComponentRecordLinksTwoCatalogs() =
        component("android.component.record-links.two-catalogs", wide = true) {
            Links(BridgeCatalog.MUSIC_BRAINZ, BridgeCatalog.DISCOGS)
        }

    @Test
    fun androidComponentRecordLinksWrapping() =
        component("android.component.record-links.wrapping", wide = true) {
            Links(
                BridgeCatalog.MUSIC_BRAINZ,
                BridgeCatalog.DISCOGS,
                BridgeCatalog.BANDCAMP,
                BridgeCatalog.WIKIDATA,
                BridgeCatalog.APPLE_MUSIC,
            )
        }

    @Test
    fun androidComponentReleasePickerTwoReleases() =
        component("android.component.release-picker.two-releases", wide = true) { Picker(shown = 0) }

    @Test
    fun androidComponentReleasePickerSecondShown() =
        component("android.component.release-picker.second-shown", wide = true) { Picker(shown = 1) }

    @Test
    fun androidComponentAlbumCreditsCredits() =
        component("android.component.album-credits.credits", wide = true) {
            AndroidAlbumCredits(
                listOf(credit("Producer", "Ines Varga"), credit("Engineer", "Pell Marsh"), credit("Arranger", "Ada Lindqvist")),
            )
        }

    @Test
    fun androidComponentAlbumCreditsLongNames() =
        component("android.component.album-credits.long-names", wide = true) {
            AndroidAlbumCredits(
                listOf(
                    credit("Producer", "Ines Varga and Tomas Brell"),
                    credit("Mastering engineer", "Pell Marsh at Harbor Room Studios"),
                ),
            )
        }

    @Test
    fun androidComponentArtistOtherNamesOneName() =
        component("android.component.artist-other-names.one-name", wide = true) { AndroidArtistOtherNames(listOf("Slow Orchards")) }

    @Test
    fun androidComponentArtistOtherNamesTwoNames() =
        component("android.component.artist-other-names.two-names", wide = true) {
            AndroidArtistOtherNames(listOf("Slow Orchards", "The Slow Orchards Band"))
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

@Composable
private fun Picker(shown: Int) {
    val releases =
        listOf("CD", "LP · 2014 reissue").mapIndexed { index, name ->
            PreviewData.release(id = "rel-$index").copy(name = BridgeReleaseName.Named(name))
        }
    val detail: BridgeAlbumDetail =
        PreviewData.albumDetail(releases = releases).copy(shownRelease = releases[shown], shownReleaseIndex = shown.toUInt())
    AndroidReleasePicker(detail, onSelect = {})
}

private fun credit(
    role: String,
    name: String,
) = BridgeCreditRow(
    role = BridgeCreditRole(BridgeTermLabel.Verbatim(role), emptyList()),
    artists = creditLineNamed(name),
    tracks = emptyList(),
)

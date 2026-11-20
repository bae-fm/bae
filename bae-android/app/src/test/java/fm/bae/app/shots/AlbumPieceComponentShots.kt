package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.BridgeFixtures
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.creditLineNamed
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.data.TestImages
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumCredits
import fm.bae.app.ui.albumdetail.AndroidAlbumRating
import fm.bae.app.ui.albumdetail.AndroidRecordLinks
import fm.bae.app.ui.albumdetail.AndroidReleasePicker
import fm.bae.app.ui.albumdetail.GalleryViewer
import fm.bae.app.ui.library.AndroidArtistOtherNames
import fm.bae.app.ui.library.AndroidPlaylistHeader
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumRating
import uniffi.bae_bridge.BridgeCatalog
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeCreditRow
import uniffi.bae_bridge.BridgeGalleryLabel
import uniffi.bae_bridge.BridgePersonRef
import uniffi.bae_bridge.BridgePlaylistCover
import uniffi.bae_bridge.BridgePlaylistSummary
import uniffi.bae_bridge.BridgeRater
import uniffi.bae_bridge.BridgeReleaseName
import uniffi.bae_bridge.BridgeReleaseRecord
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.BridgeStars
import uniffi.bae_bridge.BridgeTermLabel

/**
 * The component shots of the album page's pieces: rating, record links, release picker, lightbox, credits, and the artist page's
 * other names.
 */
class AlbumPieceComponentShots : BoardShot() {
    @Test
    fun androidComponentAlbumRatingNotRated() = component("android.component.album-rating.not-rated") { Rating(mine = null) }

    @Test
    fun androidComponentAlbumRatingRated() = component("android.component.album-rating.rated") { Rating(BridgeStars.FOUR, BridgeRater.You) }

    @Test
    fun androidComponentAlbumRatingRatedByOthers() =
        component("android.component.album-rating.rated-by-others") { Rating(BridgeStars.FOUR, BridgeRater.You, anaRatedFive) }

    @Test
    fun androidComponentAlbumRatingOthersOnly() = component("android.component.album-rating.others-only") { Rating(null, anaRatedFive) }

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
    fun androidComponentLightboxImage() = shot("android.component.lightbox.image") { Lightbox(shownImage(), count = 4u) }

    @Test
    fun androidComponentLightboxLoading() = shot("android.component.lightbox.loading") { Lightbox(neverLoaded(), count = 4u) }

    @Test
    fun androidComponentLightboxOneImage() = shot("android.component.lightbox.one-image") { Lightbox(shownImage(), count = 1u) }

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

    @Test
    fun androidComponentPlaylistHeaderPlaylist() =
        component("android.component.playlist-header.playlist", wide = true) {
            Header(
                BridgeFixtures.playlistSummary(name = "Late night", sharing = BridgeSharing.HOUSEHOLD).copy(
                    owner = BridgePersonRef(person = "person-1", name = "Ana", initials = "A", avatarVersion = null),
                    cover = BridgePlaylistCover.Grid(covers = List(4) { PreviewData.imageRef("img-$it") }),
                ),
            )
        }

    @Test
    fun androidComponentPlaylistHeaderLongName() =
        component("android.component.playlist-header.long-name", wide = true) {
            Header(
                BridgeFixtures.playlistSummary(name = "Sunday mornings in the kitchen").copy(
                    owner = BridgePersonRef(person = "person-2", name = "Ben", initials = "B", avatarVersion = null),
                    trackCount = BridgeCountLabel(key = "core.library.track_count", count = 1uL),
                    cover = BridgePlaylistCover.Single(PreviewData.imageRef("img-1")),
                ),
            )
        }
}

/** The lightbox on one gallery item, the release's cover.jpg, of [count] images, reading its bytes from [store]. */
@Composable
private fun Lightbox(
    store: ImageStore,
    count: UInt,
) = ScreenScene {
    CompositionLocalProvider(LocalImageStore provides store, LocalBugReports provides BugReports()) {
        GalleryViewer(
            releaseId = "rel-1",
            items = listOf(PreviewData.galleryItem(label = BridgeGalleryLabel.File("cover.jpg"))),
            count = count,
            onDismiss = {},
        )
    }
}

/** A store whose release image is a picture. */
private fun shownImage() =
    ImageStore(
        fetchReleaseImageBytes = {
            _,
            _,
            ->
            TestImages.quadrantsJpeg(400, listOf(0xFF2D4A6B.toInt(), 0xFF6B8EAD.toInt(), 0xFFC9A66B.toInt(), 0xFF8C3B4A.toInt()))
        },
        dispatcher = Dispatchers.Unconfined,
    )

/** A store whose bytes never arrive, so the lightbox stays on its load. */
private fun neverLoaded() = ImageStore(fetchReleaseImageBytes = { _, _ -> awaitCancellation() }, dispatcher = Dispatchers.Unconfined)

/** The album's stars at [mine], over the line that names the [raters]. */
@Composable
private fun Rating(
    mine: BridgeStars?,
    vararg raters: BridgeRater,
) = AndroidAlbumRating(BridgeAlbumRating(mine = mine, raters = raters.toList()), onRate = {})

private val anaRatedFive = BridgeRater.Person("Ana", BridgeStars.FIVE)

@Composable
private fun Header(summary: BridgePlaylistSummary) = Images { AndroidPlaylistHeader(summary, onPlay = {}, onShuffle = {}) }

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

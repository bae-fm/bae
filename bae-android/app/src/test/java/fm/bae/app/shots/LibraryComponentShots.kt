package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.albumdetail.AndroidTrackRow
import fm.bae.app.ui.albumdetail.TrackRowCallbacks
import fm.bae.app.ui.albumdetail.TrackRowData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidAlbumTile
import fm.bae.app.ui.library.AndroidArtistHeading
import fm.bae.app.ui.library.AndroidMenuItem
import fm.bae.app.ui.library.AndroidSearchSection
import fm.bae.app.ui.library.GroupByArtistItem
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeFactTerm
import uniffi.bae_bridge.BridgeLabelLine
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeMediaCount
import uniffi.bae_bridge.BridgeMedium
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeTermLabel

private val callbacks = TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {})
private val section = BridgeLibraryBrowseSection("artist-1", "Mara Quill", null, 0uL, 12uL)

/** The component shots of the library's parts: the album grid and its tiles, the artist heading, the album header and track rows. */
class LibraryComponentShots : BoardShot() {
    @Test
    fun androidComponentAlbumTileDefault() =
        component("android.component.album-tile.default") {
            Images {
                Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.edge)) {
                    Box(Modifier.width(160.dp)) {
                        AndroidAlbumTile("Low Tide Hours", "Mara Quill", PreviewData.imageRef(), onClick = {})
                    }
                    Box(Modifier.width(160.dp)) { AndroidAlbumTile("Copper Sky", "1972", null, onClick = {}) }
                }
            }
        }

    @Test
    fun androidComponentAlbumGridDefault() =
        component("android.component.album-grid.default", wide = true) {
            Images {
                val rows = previewAlbums
                Box(Modifier.width(380.dp).height(240.dp)) {
                    AndroidAlbumGrid(
                        count = rows.size,
                        sections = emptyList(),
                        rowAt = rows::get,
                        gridState = rememberLazyGridState(),
                        onSelectAlbum = {},
                        onSelectArtist = {},
                    )
                }
            }
        }

    @Test
    fun androidComponentArtistHeadingTop() =
        component("android.component.artist-heading.top", wide = true) {
            Images { AndroidArtistHeading(section, pinned = false, gridPadding = 16.dp, onClick = {}) }
        }

    @Test
    fun androidComponentArtistHeadingPinned() =
        component("android.component.artist-heading.pinned", wide = true) {
            Images { AndroidArtistHeading(section, pinned = true, gridPadding = 16.dp, onClick = {}) }
        }

    @Test
    fun androidComponentAlbumHeaderDefault() =
        component("android.component.album-header.default", wide = true) {
            Images {
                AndroidAlbumHeader(
                    detail = harborDetail,
                    selectedRelease = harborRelease,
                    cover = PreviewData.imageRef(),
                    galleryItems = emptyList(),
                    onShowGallery = {},
                )
            }
        }

    @Test
    fun androidComponentAlbumHeaderAReissue() =
        component("android.component.album-header.a-reissue", wide = true) {
            Images {
                AndroidAlbumHeader(
                    detail = harborDetail,
                    selectedRelease = harborRelease.copy(distinctYear = 2001),
                    cover = PreviewData.imageRef(),
                    galleryItems = emptyList(),
                    onShowGallery = {},
                )
            }
        }

    @Test
    fun androidComponentTrackRowResting() =
        component("android.component.track-row.resting", wide = true) {
            TrackRowShot(TrackRowData("1", "Harbor Lights", null, "4:32", isCurrent = false, isPlaying = false))
        }

    @Test
    fun androidComponentTrackRowCurrent() =
        component("android.component.track-row.current", wide = true) {
            TrackRowShot(TrackRowData("2", "The Long Way Home", null, "5:12", isCurrent = true, isPlaying = true))
        }

    @Test
    fun androidComponentTrackRowCompilation() =
        component("android.component.track-row.compilation", wide = true) {
            TrackRowShot(TrackRowData("3", "Low Tide Hours", "Mara Quill", "5:03", isCurrent = false, isPlaying = false))
        }

    @Test
    fun androidComponentLibraryRowArtist() =
        component("android.component.library-row.artist", wide = true) {
            Images { AndroidLibraryRow(title = "Mara Quill", subtitle = "12 albums", art = PreviewData.imageRef()) }
        }

    @Test
    fun androidComponentLibraryRowNoPicture() =
        component("android.component.library-row.no-picture", wide = true) {
            Images { AndroidLibraryRow(title = "Odile Fen", subtitle = "3 albums", missing = true) }
        }

    @Test
    fun androidComponentLibraryRowAlbum() =
        component("android.component.library-row.album", wide = true) {
            Images { AndroidLibraryRow(title = "Low Tide Hours", subtitle = "Mara Quill · 1992", art = PreviewData.imageRef()) }
        }

    @Test
    fun androidComponentLibraryRowTrack() =
        component("android.component.library-row.track", wide = true) {
            AndroidLibraryRow(title = "Harbor Lights", subtitle = "Mara Quill, Low Tide Hours", trailing = "4:32", noArt = true)
        }

    @Test
    fun androidComponentLibraryRowWork() =
        component("android.component.library-row.work", wide = true) {
            Images {
                AndroidLibraryRow(title = "Low Tide Suite", subtitle = "Ada Lindqvist", art = PreviewData.imageRef(), inset = true)
            }
        }

    @Test
    fun androidComponentLibraryRowPart() =
        component("android.component.library-row.part", wide = true) {
            Images {
                AndroidLibraryRow(
                    title = "II. Slack Water",
                    subtitle = "Ada Lindqvist",
                    art = PreviewData.imageRef(),
                    part = true,
                    inset = true,
                )
            }
        }

    @Test
    fun androidComponentSearchSectionDefault() =
        component("android.component.search-section.default", wide = true) {
            AndroidSearchSection("Albums")
        }

    @Test
    fun androidComponentMenuItemDefault() =
        component("android.component.menu-item.default") {
            // The menu's own window isn't drawn in a test: its surface is drawn here.
            Surface(color = MenuDefaults.containerColor, shape = MenuDefaults.shape) {
                Column {
                    AndroidMenuItem("Albums", selected = true, onClick = {})
                    AndroidMenuItem("Artists", selected = false, onClick = {})
                    AndroidMenuItem("Composers (pressed)", selected = false, onClick = {})
                    GroupByArtistItem(grouped = false, onChange = {})
                    GroupByArtistItem(grouped = true, onChange = {})
                }
            }
        }
}

/** Low Tide Hours as the design system's previews pressed it: a 1992 CD on Harbor Records. */
internal val harborRelease: BridgeRelease =
    PreviewData.release().let { release ->
        release.copy(
            distinctYear = null,
            labels = listOf(BridgeLabelLine(names = listOf("Harbor Records"), catalogNumbers = listOf("HR-1042"))),
            facts = release.facts.copy(area = null, media = listOf(BridgeMediaCount(BridgeMedium.CD, 1u))),
            pressingSummary = listOf(BridgeFactTerm.Worded(BridgeTermLabel.Verbatim("CD"))),
        )
    }

internal val harborDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        PreviewData.album(title = "Low Tide Hours", year = 1992, artistNames = "Mara Quill"),
        releases = listOf(harborRelease),
    )

/** Low Tide Hours as the detail screen's preview draws its header: the title, artist and year. */
internal val plainDetail: BridgeAlbumDetail =
    PreviewData.albumDetail(
        harborDetail.album,
        releases = listOf(harborRelease.copy(labels = emptyList(), pressingSummary = emptyList())),
    )

/** The two albums the album grid's and library screen's previews hold. */
internal val previewAlbums =
    listOf(
        BridgeAlbumAppearance(PreviewData.album(id = "a1", title = "Low Tide Hours", artistNames = "Mara Quill"), null),
        BridgeAlbumAppearance(PreviewData.album(id = "a2", title = "Paper Lanterns", artistNames = "The Slow Orchards"), null),
    )

@Composable
private fun TrackRowShot(data: TrackRowData) = AndroidTrackRow(data, callbacks, positionColumnWidth = 40.dp)

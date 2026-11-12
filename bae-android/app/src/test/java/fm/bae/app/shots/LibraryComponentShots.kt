package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidAlbumHeader
import fm.bae.app.ui.albumdetail.AndroidTrackRow
import fm.bae.app.ui.albumdetail.TrackRowCallbacks
import fm.bae.app.ui.albumdetail.TrackRowData
import fm.bae.app.ui.components.AndroidLibraryRow
import fm.bae.app.ui.library.AndroidAlbumGrid
import fm.bae.app.ui.library.AndroidAlbumTile
import fm.bae.app.ui.library.AndroidArtistHeading
import fm.bae.app.ui.library.AndroidMenuItem
import fm.bae.app.ui.library.AndroidSearchSection
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibraryPageWindow

private val callbacks = TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {})
private val section = BridgeLibraryBrowseSection("artist-1", "Mara Quill", null, BridgeLibraryPageWindow(0uL, 3uL))

/** The component shots of the library's parts: the album grid and its tiles, the artist heading, the album header and track rows. */
class LibraryComponentShots : BoardShot() {
    @Test
    fun androidComponentAlbumTileDefault() =
        component("android.component.album-tile.default") {
            Images {
                Box(Modifier.width(160.dp)) {
                    AndroidAlbumTile("Low Tide Hours", "Mara Quill", PreviewData.imageRef(), onClick = {})
                }
            }
        }

    @Test
    fun androidComponentAlbumGridDefault() =
        component("android.component.album-grid.default", wide = true) {
            Images {
                val rows =
                    listOf(
                        BridgeAlbumAppearance(PreviewData.album(id = "a1", title = "Low Tide Hours", artistNames = "Mara Quill"), null),
                        BridgeAlbumAppearance(
                            PreviewData.album(id = "a2", title = "Paper Lanterns", artistNames = "The Slow Orchards"),
                            null,
                        ),
                    )
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
                    detail = PreviewData.albumDetail(PreviewData.album(title = "Low Tide Hours", artistNames = "Mara Quill")),
                    selectedRelease = PreviewData.release(),
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
            Column {
                AndroidMenuItem("Album", selected = true, onClick = {})
                AndroidMenuItem("Artist", selected = false, onClick = {})
            }
        }
}

@Composable
private fun TrackRowShot(data: TrackRowData) = AndroidTrackRow(data, callbacks, positionColumnWidth = 40.dp)

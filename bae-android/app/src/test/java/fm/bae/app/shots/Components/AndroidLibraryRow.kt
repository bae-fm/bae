package fm.bae.app.shots

import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.AndroidLibraryRow
import org.junit.Test

/** The component shots of `AndroidLibraryRow`, one for each row of its preview. */
class AndroidLibraryRowShots : BoardShot() {
    @Test
    fun androidComponentLibraryRowArtist() =
        component("android.component.library-row.artist") {
            Images { AndroidLibraryRow(title = "Mara Quill", subtitle = "12 albums", art = PreviewData.imageRef()) }
        }

    @Test
    fun androidComponentLibraryRowNoPicture() =
        component("android.component.library-row.no-picture") {
            Images { AndroidLibraryRow(title = "Odile Fen", subtitle = "3 albums", missing = true) }
        }

    @Test
    fun androidComponentLibraryRowAlbum() =
        component("android.component.library-row.album") {
            Images { AndroidLibraryRow(title = "Low Tide Hours", subtitle = "Mara Quill · 1992", art = PreviewData.imageRef()) }
        }

    @Test
    fun androidComponentLibraryRowTrack() =
        component("android.component.library-row.track") {
            AndroidLibraryRow(title = "Harbor Lights", subtitle = "Mara Quill, Low Tide Hours", trailing = "4:32", noArt = true)
        }

    @Test
    fun androidComponentLibraryRowWork() =
        component("android.component.library-row.work") {
            Images {
                AndroidLibraryRow(title = "Low Tide Suite", subtitle = "Ada Lindqvist", art = PreviewData.imageRef(), inset = true)
            }
        }

    @Test
    fun androidComponentLibraryRowPart() =
        component("android.component.library-row.part") {
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
}

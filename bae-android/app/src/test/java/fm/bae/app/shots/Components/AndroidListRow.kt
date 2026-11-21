package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.AndroidListRow
import fm.bae.app.ui.components.AvatarFace
import fm.bae.app.ui.components.TrackOptionsMenu
import org.junit.Test

/** The component shots of `AndroidListRow`, one for each row of its preview. */
class AndroidListRowShots : BoardShot() {
    @Test
    fun androidComponentListRowResting() = component("android.component.list-row.resting", wide = true) { Entry() }

    @Test
    fun androidComponentListRowWho() =
        component("android.component.list-row.who", wide = true) { Entry(who = AvatarFace("A", picture = null)) }

    @Test
    fun androidComponentListRowMenu() = component("android.component.list-row.menu", wide = true) { Entry(menu = true) }

    @Test
    fun androidComponentListRowDragging() =
        component("android.component.list-row.dragging", wide = true) {
            Entry(menu = true, dragging = true)
        }
}

@Composable
private fun Entry(
    who: AvatarFace? = null,
    menu: Boolean = false,
    dragging: Boolean = false,
) = Images {
    AndroidListRow(
        title = "Harbor Lights",
        subtitle = "Mara Quill · Low Tide Hours",
        trailing = "4:32",
        art = PreviewData.imageRef(),
        onClick = {},
        who = who,
        dragging = dragging,
        menu = if (menu) ({ TrackOptionsMenu(onPlayNext = {}, onAddToQueue = {}) }) else null,
    )
}

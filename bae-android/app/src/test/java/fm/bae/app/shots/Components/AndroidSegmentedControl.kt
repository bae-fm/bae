package fm.bae.app.shots

import fm.bae.app.ui.components.AndroidSegmentedControl
import org.junit.Test

/** The component shots of `AndroidSegmentedControl`, one for each row of its preview. */
class AndroidSegmentedControlShots : BoardShot() {
    @Test
    fun androidComponentSegmentedControlChoice() =
        component("android.component.segmented-control.choice") {
            AndroidSegmentedControl(options = (1..6).map { "$it" }, selected = 2, onSelect = {})
        }

    @Test
    fun androidComponentSegmentedControlWithALabel() =
        component("android.component.segmented-control.with-a-label") {
            AndroidSegmentedControl(options = listOf("Member", "Owner"), selected = 0, onSelect = {}, label = "Role")
        }

    @Test
    fun androidComponentSegmentedControlWithAHelper() =
        component("android.component.segmented-control.with-a-helper") {
            AndroidSegmentedControl(
                options = listOf("Only me", "Everyone"),
                selected = 1,
                onSelect = {},
                label = "Who sees it",
                helper = "Everyone in the library sees this playlist and can play it. Only you can change it.",
            )
        }
}

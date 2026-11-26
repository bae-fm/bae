package fm.bae.app.shots

import androidx.compose.ui.test.onRoot
import builds.shots.ElementList
import builds.shots.ReadingOrder
import fm.bae.app.ui.components.RemovalLoss
import fm.bae.app.ui.components.removeLibraryParts
import fm.bae.app.ui.onboarding.WelcomeLibrariesScene
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test

/** The element list of a screen with a dialog over it. */
class DialogReadingOrderTest : BaeBoardShot() {
    @Test
    fun theScreenUnderADialogIsListedInTheOrderItHasWithoutTheDialog() {
        val screenOrder =
            compose.settleInBaeLook {
                InPlaceDialogScene(behind = { WelcomeLibrariesScene() }) {
                    removeLibraryParts(sceneLibrary, RemovalLoss.Counted(holdsUnsent = false), {}, {})
                }
            }
        val elements =
            ElementList
                .of(
                    "android.test.dialog-over-welcome",
                    compose.onRoot().fetchSemanticsNode(),
                    embedded = emptyList(),
                    unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode(),
                    reading = ReadingOrder.of(compose.onRoot().fetchSemanticsNode().root as android.view.View, screenOrder),
                ).getJSONArray("elements")
        val labels =
            (0 until elements.length())
                .map {
                    elements.getJSONObject(
                        it,
                    )
                }.filterNot(JSONObject::labelIsNull)
                .map { it.getString("label") }

        // The dialog is a window of its own, which TalkBack reads first; the welcome under it keeps the order it has
        // with no dialog: the logo, its tagline, then the libraries.
        assertEquals(listOf("Cancel", "Remove"), labels.filter { it == "Cancel" || it == "Remove" })
        assertEquals(
            listOf("bae", "Your music, everywhere", "Your libraries", "Home music", "More", "Odile’s records", "More", "Add library"),
            labels.dropWhile { it != "bae" },
        )
    }
}

private fun JSONObject.labelIsNull() = isNull("label")

package fm.bae.app.ui.albumdetail

import android.app.Application
import android.view.View
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Text
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.shots.ElementList
import fm.bae.app.shots.ReadingOrder
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class AndroidTrackRowTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun currentTrackLeadIsAnImageOfItsOwnInTheClickableRow() {
        compose.setContent {
            TestTheme {
                AndroidTrackRow(
                    TrackRowData("4", "Low Tide Hours", null, "5:03", isCurrent = true, isPlaying = true),
                    TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {}),
                    positionColumnWidth = 40.dp,
                )
            }
        }
        // One node holds the name: the lead, as an image. The clickable row does not merge it into its own.
        compose.onAllNodesWithContentDescription("Now playing").assertCountEquals(1)
        compose
            .onNodeWithContentDescription("Now playing")
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Image))
    }

    @Test
    fun currentTrackLeadIsReadAfterTheAlbumTitleAboveTheRow() {
        compose.setContent {
            TestTheme {
                Column {
                    Text("Album title")
                    AndroidTrackRow(
                        TrackRowData("4", "Low Tide Hours", null, "5:03", isCurrent = true, isPlaying = true),
                        TrackRowCallbacks(onClick = {}, onPlayNext = {}, onAddToQueue = {}),
                        positionColumnWidth = 40.dp,
                    )
                }
            }
        }
        val root = compose.onRoot().fetchSemanticsNode()
        ReadingOrder.enableTalkBack(root)
        compose.waitForIdle()
        val elements =
            ElementList
                .of(
                    "android.test.track-row",
                    root,
                    unmerged = compose.onRoot(useUnmergedTree = true).fetchSemanticsNode(),
                    reading = ReadingOrder.of(root.root as View),
                ).getJSONArray("elements")
        val labels = (0 until elements.length()).map { elements.getJSONObject(it).optString("label") }

        // The lead's traversal index puts it first in its row, not ahead of the page above the row.
        assertEquals(
            listOf("Album title", "Now playing", "Low Tide Hours", "5:03"),
            labels.filter {
                it in
                    setOf("Album title", "Now playing", "Low Tide Hours", "5:03")
            },
        )
    }
}

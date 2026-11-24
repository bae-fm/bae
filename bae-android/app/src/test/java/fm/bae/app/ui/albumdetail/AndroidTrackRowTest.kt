package fm.bae.app.ui.albumdetail

import android.app.Application
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.TestTheme
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
}

package fm.bae.app.ui.sync

import fm.bae.app.DisplayError
import fm.bae.app.ui.ActivityProblemId
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.PreviewData
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSyncActivity

class ActivityProblemsTest {
    private val signIn =
        AppProblem(ActivityProblemId.SIGN_IN_SETTINGS, "Couldn't read the sign-in settings", null, DisplayError("x"))
    private val unread = DisplayError("Couldn’t read sync status")

    @Test
    fun problemsListInThePanelsOrder() {
        val problems = activityProblems(listOf(signIn), failed(), unread, emptySet())

        assertEquals(
            listOf(
                ActivityProblemId.SIGN_IN_SETTINGS,
                ActivityProblemId.SYNC_STATUS_UNREAD,
                ActivityProblemId.SYNC_FAILURE,
            ),
            problems.listed,
        )
        assertEquals(problems.listed, problems.shown)
    }

    @Test
    fun aDismissedProblemLeavesShownAndTheRestStay() {
        val problems =
            activityProblems(listOf(signIn), failed(), unread, setOf(ActivityProblemId.SYNC_STATUS_UNREAD))

        assertEquals(
            listOf(ActivityProblemId.SIGN_IN_SETTINGS, ActivityProblemId.SYNC_STATUS_UNREAD, ActivityProblemId.SYNC_FAILURE),
            problems.listed,
        )
        assertEquals(listOf(ActivityProblemId.SIGN_IN_SETTINGS, ActivityProblemId.SYNC_FAILURE), problems.shown)
    }

    @Test
    fun needsAttentionEndsWhenEveryListedProblemIsDismissedUnlessARecordIsListed() {
        val everything = setOf(ActivityProblemId.SIGN_IN_SETTINGS, ActivityProblemId.SYNC_FAILURE)

        assertFalse(activityProblems(listOf(signIn), failed(), null, everything).needsAttention)
        assertTrue(activityProblems(listOf(signIn), failed(), null, emptySet()).needsAttention)
        assertTrue(
            "a pending record has no dismissal",
            activityProblems(emptyList(), PreviewData.syncActivity(needsAttention = true), null, everything)
                .needsAttention,
        )
    }

    @Test
    fun aNewKindOfProblemIsShownAmongDismissedOnes() {
        val dismissed = setOf(ActivityProblemId.SIGN_IN_SETTINGS)

        assertFalse(activityProblems(listOf(signIn), settled(), null, dismissed).needsAttention)
        assertTrue(activityProblems(listOf(signIn), settled(), unread, dismissed).needsAttention)
    }

    @Test
    fun nothingListedWhileNothingIsWrong() {
        val problems = activityProblems(emptyList(), null, null, emptySet())

        assertEquals(emptyList<ActivityProblemId>(), problems.listed)
        assertFalse(problems.needsAttention)
    }

    private fun failed() = settled().copy(failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "offline"))

    private fun settled() =
        BridgeSyncActivity(
            moving = false,
            held = false,
            needsAttention = false,
            failure = null,
            attention = emptyList(),
            syncing = false,
            uploads = null,
            pins = null,
            makingLocal = null,
        )
}

package fm.bae.app.ui.sync

import fm.bae.app.DisplayError
import fm.bae.app.ui.ActivityProblemId
import fm.bae.app.ui.AppProblem
import uniffi.bae_bridge.BridgeSyncActivity

/**
 * The problems the Activity sheet lists, in the order it lists them: [listed] is every one there is,
 * [shown] the ones the person hasn't dismissed, and [recordsListed] whether pending records are listed,
 * which have no dismissal.
 */
data class ActivityProblems(
    val listed: List<ActivityProblemId>,
    val shown: List<ActivityProblemId>,
    val recordsListed: Boolean,
) {
    /** Something is listed that the person hasn't dismissed: the button shows a warning. */
    val needsAttention: Boolean get() = shown.isNotEmpty() || recordsListed
}

/**
 * The problems to list: the [app]'s own, then a failure to read sync's activity ([error]), then sync's
 * own failure and its pending records from [activity]. [dismissed] ones are listed but not shown.
 */
fun activityProblems(
    app: List<AppProblem>,
    activity: BridgeSyncActivity?,
    error: DisplayError?,
    dismissed: Set<ActivityProblemId>,
): ActivityProblems {
    val listed =
        app.map { it.id } +
            listOfNotNull(
                ActivityProblemId.SYNC_STATUS_UNREAD.takeIf { error != null },
                ActivityProblemId.SYNC_FAILURE.takeIf { activity?.failure != null },
            )
    return ActivityProblems(
        listed = listed,
        shown = listed.filter { it !in dismissed },
        recordsListed = activity?.attention?.isNotEmpty() == true,
    )
}

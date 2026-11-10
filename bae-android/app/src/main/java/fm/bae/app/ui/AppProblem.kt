package fm.bae.app.ui

import android.content.Context
import fm.bae.app.DisplayError
import fm.bae.app.coreString

/** The problems the Activity sheet lists, each of which the person can dismiss from it. */
enum class ActivityProblemId { SIGN_IN_SETTINGS, SYNC_STATUS_UNREAD, SYNC_FAILURE }

/**
 * A problem of the app's own that the Activity sheet lists while a library is open: [title] says
 * what failed, [advice] what to do about it, and [error] is what the notice's dialog explains.
 */
data class AppProblem(
    val id: ActivityProblemId,
    val title: String,
    val advice: String?,
    val error: DisplayError,
)

/** This build's Dropbox and OneDrive sign-in settings couldn't be read, for the reason [error] names. */
fun signInSettingsProblem(
    context: Context,
    error: String,
): AppProblem =
    AppProblem(
        id = ActivityProblemId.SIGN_IN_SETTINGS,
        title = context.coreString("ui.load_error.sign_in_settings.title"),
        advice = context.coreString("ui.load_error.sign_in_settings.advice"),
        error = DisplayError(error),
    )

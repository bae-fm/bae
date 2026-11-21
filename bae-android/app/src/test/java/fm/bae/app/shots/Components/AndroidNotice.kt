package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.DisplayError
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeButtons
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.StatusTone
import org.junit.Test

/** The component shots of `AndroidNotice`, one for each row of its preview. */
class AndroidNoticeShots : BoardShot() {
    @Test
    fun androidComponentNoticeNeutral() =
        component("android.component.notice.neutral", wide = true) { Titled("This library has no albums yet.", StatusTone.NEUTRAL) }

    @Test
    fun androidComponentNoticeInfo() =
        component("android.component.notice.info", wide = true) {
            Titled("A newer version of this release is on Discogs.", StatusTone.INFO)
        }

    @Test
    fun androidComponentNoticeSuccess() =
        component("android.component.notice.success", wide = true) { Titled("Cloud connected.", StatusTone.SUCCESS) }

    @Test
    fun androidComponentNoticeWarning() =
        component("android.component.notice.warning", wide = true) { Titled("Unsent changes will be lost", StatusTone.WARNING) }

    @Test
    fun androidComponentNoticeWarningTitleAndSubtitle() =
        component("android.component.notice.warning-title-and-subtitle", wide = true) {
            Titled("Mara Quill’s phone is out of date", StatusTone.WARNING, advice = "Update bae on it to keep syncing.")
        }

    @Test
    fun androidComponentNoticeDanger() =
        component("android.component.notice.danger", wide = true) { Titled("Could not reach the cloud.", StatusTone.DANGER) }

    @Test
    fun androidComponentNoticeDangerRetry() =
        component("android.component.notice.danger-retry", wide = true) {
            Titled("Could not reach the cloud.", StatusTone.DANGER, action = ErrorAction("Retry") {})
        }

    @Test
    fun androidComponentNoticeDangerDetailsAndRetry() =
        component("android.component.notice.danger-details-and-retry", wide = true) {
            AndroidNotice(error = CLOUD_ERROR, action = ErrorAction("Retry") {})
        }

    @Test
    fun androidComponentNoticeDangerActionsBelowLongText() =
        component("android.component.notice.danger-actions-below-long-text", wide = true) {
            AndroidNotice(
                error =
                    DisplayError(
                        "Could not reach the cloud. Your changes stay on this device and go up once it is back; " +
                            "nothing is lost in the meantime.",
                        "dns error",
                    ),
                action = ErrorAction("Retry") {},
                buttons = NoticeButtons.BELOW,
            )
        }

    @Test
    fun androidComponentNoticeDangerTitleAndSubtitle() =
        component("android.component.notice.danger-title-and-subtitle", wide = true) {
            Titled(
                "bae could not open your library",
                StatusTone.DANGER,
                advice = "Check your connection, then quit and reopen bae.",
            )
        }

    @Test
    fun androidComponentNoticeWarningQuietActionsBelow() =
        component("android.component.notice.warning-quiet-actions-below", wide = true) {
            AndroidNotice(
                title = "Dropbox signed this device out.",
                advice = null,
                error = null,
                tone = StatusTone.WARNING,
                look = NoticeLook.QUIET,
                action = ErrorAction("Open Sync Settings") {},
                onDismiss = {},
                buttons = NoticeButtons.BELOW,
            )
        }

    @Test
    fun androidComponentNoticeDangerQuietWith() =
        component("android.component.notice.danger-quiet-with", wide = true) {
            AndroidNotice(
                title = "Could not close the library",
                advice = null,
                error = null,
                tone = StatusTone.DANGER,
                look = NoticeLook.QUIET,
                onDismiss = {},
            )
        }

    @Test
    fun androidComponentNoticeDangerCentredAFailedView() =
        component("android.component.notice.danger-centred-a-failed-view", wide = true) {
            AndroidNotice(
                title = "Could not read your library",
                advice = null,
                error = DisplayError("Could not read your library", "disk I/O error"),
                look = NoticeLook.CENTERED,
                action = ErrorAction("Retry") {},
            )
        }
}

private val CLOUD_ERROR = DisplayError("Could not reach the cloud.", "dns error: failed to lookup address information")

@Composable
private fun Titled(
    title: String,
    tone: StatusTone,
    advice: String? = null,
    action: ErrorAction? = null,
) = AndroidNotice(title = title, advice = advice, error = null, tone = tone, action = action)

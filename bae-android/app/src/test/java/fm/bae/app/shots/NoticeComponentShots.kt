package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.ui.appearance.AccentChoice
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDisclosure
import fm.bae.app.ui.components.AndroidErrorDetails
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeButtons
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.AndroidErrorBanner
import fm.bae.app.ui.settings.AndroidAccentPicker
import org.junit.Test

/** The component shots of the notices and what hangs off them: the notice, its error details and banner, notes, disclosures and the accent picker. */
class NoticeComponentShots : BoardShot() {
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
        component("android.component.notice.danger", wide = true) { Titled("Couldn’t reach the cloud.", StatusTone.DANGER) }

    @Test
    fun androidComponentNoticeDangerRetry() =
        component("android.component.notice.danger-retry", wide = true) {
            Titled("Couldn’t reach the cloud.", StatusTone.DANGER, action = ErrorAction("Retry") {})
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
                        "Couldn’t reach the cloud. Your changes stay on this device and go up once it’s back; " +
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
                "bae couldn’t open your library",
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
                title = "Couldn’t close the library",
                advice = null,
                error = null,
                tone = StatusTone.DANGER,
                look = NoticeLook.QUIET,
                onDismiss = {},
            )
        }

    @Test
    fun androidComponentNoticeDangerCentred() =
        component("android.component.notice.danger-centred", wide = true) {
            AndroidNotice(
                title = "Couldn’t read your library",
                advice = null,
                error = DisplayError("Couldn’t read your library", "disk I/O error"),
                look = NoticeLook.CENTERED,
                action = ErrorAction("Retry") {},
            )
        }

    @Test
    fun androidComponentErrorDetailsCauses() =
        component("android.component.error-details.causes") {
            AndroidErrorDetails(
                text = "The sign-in settings file is missing.\nOAuthConfig: missing client id",
                copyText = "The sign-in settings file is missing.\nOAuthConfig: missing client id",
                modifier = Modifier.width(320.dp),
            )
        }

    @Test
    fun androidComponentErrorBannerDefault() =
        component("android.component.error-banner.default", wide = true) {
            AndroidErrorBanner(error = DisplayError("Couldn’t read more albums."), onRetry = {})
        }

    @Test
    fun androidComponentNoteDefault() =
        component("android.component.note.default", wide = true) {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                AndroidCard { AndroidRow(title = "Restore code", subtitle = "Shown") }
                AndroidNote("Anyone with this code can open the library.")
            }
        }

    @Test
    fun androidComponentDisclosureClosed() =
        component("android.component.disclosure.closed", wide = true) {
            AndroidDisclosure(label = "Removed devices (2)", open = false, onToggle = {}) {}
        }

    @Test
    fun androidComponentDisclosureOpen() =
        component("android.component.disclosure.open", wide = true) {
            AndroidDisclosure(label = "Removed devices (2)", open = true, onToggle = {}) {
                AndroidRow(title = "Odile’s phone", subtitle = "Removed")
                AndroidRow(title = "Kitchen tablet", subtitle = "Replaced")
            }
        }

    @Test
    fun androidComponentAccentPickerResting() =
        component("android.component.accent-picker.resting", wide = true) {
            AndroidCard {
                Box(Modifier.fillMaxWidth().padding(ThemeSpace.edge)) {
                    AndroidAccentPicker(AccentChoice.PINK) {}
                }
            }
        }
}

private val CLOUD_ERROR = DisplayError("Couldn’t reach the cloud.", "dns error: failed to lookup address information")

@Composable
private fun Titled(
    title: String,
    tone: StatusTone,
    advice: String? = null,
    action: ErrorAction? = null,
) = AndroidNotice(title = title, advice = advice, error = null, tone = tone, action = action)

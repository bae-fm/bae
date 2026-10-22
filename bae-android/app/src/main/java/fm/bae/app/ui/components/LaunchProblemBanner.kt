package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText

/**
 * What launch found wrong that doesn't stop the app: this build's Dropbox and
 * OneDrive sign-in settings couldn't be read. Launch records it once, apart
 * from every screen's own failures, and it shows over every screen for the
 * run, so no later failure or opened library hides it.
 */
@Composable
fun LaunchProblemBanner(signInSettingsError: String?) {
    val detail = signInSettingsError ?: return
    val context = LocalContext.current
    Notice(
        tone = StatusTone.DANGER,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
            Text(text = context.coreString("ui.load_error.sign_in_settings.title"), style = ThemeText.heading.style)
            ErrorDetailDisclosure(
                error = DisplayError(context.coreString("ui.load_error.sign_in_settings.advice"), detail),
                lineColor = LocalContentColor.current,
            )
        }
    }
}

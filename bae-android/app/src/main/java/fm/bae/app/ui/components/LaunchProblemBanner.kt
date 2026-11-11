package fm.bae.app.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.appearance.ThemeSpace

/**
 * What launch found wrong that doesn't stop the app, such as this build's Dropbox and OneDrive
 * sign-in settings not reading. Launch records it once, apart from every screen's own failures, and
 * it shows above every screen that has no open library for the run. An open library lists it in
 * its Activity sheet instead.
 */
@Composable
fun LaunchProblemBanner(problems: List<AppProblem>) {
    Column {
        problems.forEach { problem ->
            AndroidNotice(
                title = problem.title,
                advice = problem.advice,
                error = problem.error,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
            )
        }
    }
}

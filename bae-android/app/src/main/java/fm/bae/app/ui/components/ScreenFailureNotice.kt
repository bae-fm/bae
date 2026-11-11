package fm.bae.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.ScreenFailure
import fm.bae.app.coreString

/** A failure as a screen shows it: what failed and what to do next, its error behind Details. */
@Composable
fun ScreenFailureNotice(
    failure: ScreenFailure,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    AndroidNotice(
        title = context.coreString(failure.step.titleKey),
        advice = failure.step.adviceKey?.let(context::coreString),
        error = failure.error,
        modifier = modifier,
    )
}

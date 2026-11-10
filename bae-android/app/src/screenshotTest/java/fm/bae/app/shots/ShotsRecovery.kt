package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.FailedStep
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.ScreenFailure
import fm.bae.app.ui.ResetOfferedScreen
import fm.bae.app.ui.ResetUnfinishedScreen

/** The reset-offered screen as a scene: nothing on it is tapped. */
@Composable
private fun ResetOfferedScene(failure: ScreenFailure) {
    ResetOfferedScreen(
        libraryId = "home",
        error = failure,
        prepare = { error("a screenshot scene was tapped") },
        errors = LocaleErrorDisplays(LocalContext.current),
        onReset = {},
        onLeave = {},
    )
}

private val sceneResetRequired =
    ScreenFailure(
        FailedStep.LIBRARY_OPEN,
        DisplayError("This device must be reset before syncing.", "this installation is another copy of a device"),
    )

/** Opening found this device's copy unusable: why, centred, with Reset this device…. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ResetOffered() {
    ScreenScene { ResetOfferedScene(sceneResetRequired) }
}

/** The reset offered, in light. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_NO)
@Composable
fun ResetOfferedLight() {
    ScreenScene { ResetOfferedScene(sceneResetRequired) }
}

/** A reset that failed: what failed, with Details and Retry. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ResetFailed() {
    val failure =
        ScreenFailure(
            FailedStep.DEVICE_RESET,
            DisplayError("Couldn’t reach the cloud.", "dns error: failed to lookup address information"),
        )
    ScreenScene { ResetOfferedScene(failure) }
}

/**
 * A reset that stopped partway, whose finishing failed: what failed, with Details and Finish resetting. The state with
 * no failure words its line through the bridge, which the screenshot tests can't load.
 */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ResetUnfinished() {
    val failure =
        ScreenFailure(
            FailedStep.DEVICE_RESET,
            DisplayError("Couldn’t reach the cloud.", "dns error: failed to lookup address information"),
        )
    ScreenScene { ResetUnfinishedScreen(error = failure, onFinish = {}, onLeave = {}) }
}

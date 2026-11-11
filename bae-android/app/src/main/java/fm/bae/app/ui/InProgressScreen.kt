package fm.bae.app.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.AppScreen
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidProgress
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.components.stepFraction
import fm.bae.app.ui.onboarding.ResettingScreen
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeClosingStep
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.BridgeSetupProgress
import uniffi.bae_bridge.BridgeSetupStep
import uniffi.bae_bridge.bridgeClosingStepKey
import uniffi.bae_bridge.bridgeSetupStepKey

/** The screen of a call in flight, showing how far it has come. */
@Composable
internal fun InProgressScreen(screen: AppScreen.InProgress) {
    when (screen) {
        is AppScreen.Loading -> {
            LoadingScreen(steps = null, title = screen.work.title())
        }

        is AppScreen.Opening -> {
            LoadingScreen(steps = screen.steps, title = R.string.loading_opening_library)
        }

        is AppScreen.Closing -> {
            ClosingScreen(steps = screen.steps, title = screen.work.title())
        }

        is AppScreen.FinishingRemovals -> {
            SetupStepScreen(steps = screen.steps, title = R.string.loading_removing_library)
        }

        is AppScreen.Resetting -> {
            ResettingScreen(status = screen.status, steps = screen.steps)
        }
    }
}

/** The words that name what a loading screen waits on. */
@StringRes
private fun AppScreen.LoadingWork.title(): Int =
    when (this) {
        AppScreen.LoadingWork.FINDING_LIBRARIES -> R.string.loading_finding_libraries
        AppScreen.LoadingWork.OPENING -> R.string.loading_opening_library
        AppScreen.LoadingWork.CLOSING -> R.string.loading_closing_library
        AppScreen.LoadingWork.REMOVING -> R.string.loading_removing_library
    }

/**
 * What a loading screen waits on, named by [title] under a spinner, until a
 * library's open runs a step: then [steps]'s step over its bar, in its place.
 */
@Composable
private fun LoadingScreen(
    steps: LatestSource<BridgeOpenProgress>?,
    @StringRes title: Int,
) {
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        OpenProgressLine(steps = steps, modifier = Modifier.widthIn(max = OPEN_PROGRESS_MAX_WIDTH)) {
            AndroidSpinner(text = stringResource(title))
        }
    }
}

/** How wide an open's line is where it stands alone, on the loading screen. */
private val OPEN_PROGRESS_MAX_WIDTH = 360.dp

/**
 * The open library closing or being removed: the step it runs over a bar
 * without a count, drawn from [steps] each display frame, and, while it runs
 * no step, [title], what the screen waits on, under a spinner. [stepKey]
 * gives a step's catalog key.
 */
@Composable
internal fun ClosingScreen(
    steps: LatestSource<BridgeClosingStep>,
    @StringRes title: Int,
    stepKey: (BridgeClosingStep) -> String = ::bridgeClosingStepKey,
) {
    val context = LocalContext.current
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        AndroidProgress(
            steps = steps,
            title = { context.coreString(stepKey(it)) },
            fraction = { null },
            modifier = Modifier.widthIn(max = OPEN_PROGRESS_MAX_WIDTH),
        ) {
            AndroidSpinner(text = stringResource(title))
        }
    }
}

/**
 * A setup call running: the step it runs over a bar of how much of it is
 * done, counted where the step counts, drawn from [steps] each display frame,
 * and, while the call runs no step, [title], what the screen waits on, under
 * a spinner. [stepKey] gives a step's catalog key.
 */
@Composable
internal fun SetupStepScreen(
    steps: LatestSource<BridgeSetupProgress>,
    @StringRes title: Int,
    stepKey: (BridgeSetupStep) -> String = ::bridgeSetupStepKey,
) {
    val context = LocalContext.current
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        AndroidProgress(
            steps = steps,
            title = { context.coreString(stepKey(it.step)) },
            fraction = { stepFraction(it.done, it.total)?.toDouble() },
            modifier = Modifier.widthIn(max = OPEN_PROGRESS_MAX_WIDTH),
        ) {
            AndroidSpinner(text = stringResource(title))
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun LoadingScreenPreview() {
    BaeTheme {
        LoadingScreen(steps = null, title = R.string.loading_finding_libraries)
    }
}

@Preview(showBackground = true)
@Composable
private fun OpeningScreenPreview() {
    BaeTheme {
        LoadingScreen(
            steps = LatestStream.openProgress(BridgeOpenProgress(BridgeOpenStep.MIGRATING, 2uL, 5uL)).only,
            title = R.string.loading_opening_library,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun FinishingRemovalsPreview() {
    BaeTheme {
        SetupStepScreen(
            steps =
                LatestStream
                    .setupProgress(BridgeSetupProgress(BridgeSetupStep.FINISHING_REMOVALS, 1uL, 2uL))
                    .only,
            title = R.string.loading_removing_library,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ClosingScreenPreview() {
    BaeTheme {
        ClosingScreen(
            steps = LatestStream.closingSteps(BridgeClosingStep.STOPPING_SYNC).only,
            title = R.string.loading_closing_library,
        )
    }
}

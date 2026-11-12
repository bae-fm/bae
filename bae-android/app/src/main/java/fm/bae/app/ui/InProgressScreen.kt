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
import uniffi.bae_bridge.BridgeClosingProgress
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeSetupProgress

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
 * no step, [title], what the screen waits on, under a spinner.
 */
@Composable
internal fun ClosingScreen(
    steps: LatestSource<BridgeClosingProgress>,
    @StringRes title: Int,
) {
    val context = LocalContext.current
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        AndroidProgress(
            steps = steps,
            title = { context.coreString(it.key) },
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
 * a spinner.
 */
@Composable
internal fun SetupStepScreen(
    steps: LatestSource<BridgeSetupProgress>,
    @StringRes title: Int,
) {
    val context = LocalContext.current
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        AndroidProgress(
            steps = steps,
            title = { context.coreString(it.key) },
            fraction = { stepFraction(it.done, it.total)?.toDouble() },
            modifier = Modifier.widthIn(max = OPEN_PROGRESS_MAX_WIDTH),
        ) {
            AndroidSpinner(text = stringResource(title))
        }
    }
}

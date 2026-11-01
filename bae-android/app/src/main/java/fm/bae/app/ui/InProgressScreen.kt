package fm.bae.app.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.AppScreen
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.onboarding.ResettingScreen
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.changes
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
        is AppScreen.Loading -> LoadingScreen(steps = null, title = screen.work.title())
        is AppScreen.Opening -> LoadingScreen(steps = screen.steps, title = R.string.loading_opening_library)
        is AppScreen.Closing -> ClosingScreen(steps = screen.steps)
        is AppScreen.FinishingRemovals -> SetupStepScreen(steps = screen.steps)
        is AppScreen.Resetting -> ResettingScreen(status = screen.status)
    }
}

/** The words that name what a loading screen waits on. */
@StringRes
private fun AppScreen.LoadingWork.title(): Int =
    when (this) {
        AppScreen.LoadingWork.FINDING_LIBRARIES -> R.string.loading_finding_libraries
        AppScreen.LoadingWork.OPENING -> R.string.loading_opening_library
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
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                CircularProgressIndicator()
                Text(
                    text = stringResource(title),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** How wide an open's line is where it stands alone, on the loading screen. */
private val OPEN_PROGRESS_MAX_WIDTH = 360.dp

/**
 * The open library closing: the step the close runs, read from [steps] as it
 * changes, a handful of times in all, under a spinner.
 */
@Composable
private fun ClosingScreen(steps: LatestSource<BridgeClosingStep>) {
    val context = LocalContext.current
    val step by remember(steps) { steps.changes() }.collectAsState(initial = null)
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            CircularProgressIndicator()
            step?.let {
                Text(
                    text = context.coreString(bridgeClosingStepKey(it)),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * A setup call running: the step it runs, read from [steps] as it changes,
 * over a bar of how much of it is done, counted where the step counts.
 */
@Composable
private fun SetupStepScreen(steps: LatestSource<BridgeSetupProgress>) {
    val context = LocalContext.current
    val progress by remember(steps) { steps.changes() }.collectAsState(initial = null)
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        Column(
            modifier = Modifier.widthIn(max = OPEN_PROGRESS_MAX_WIDTH),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            val shown = progress
            if (shown == null) {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.CenterHorizontally))
            } else {
                Text(
                    text = context.coreString(bridgeSetupStepKey(shown.step)),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (shown.total > 0uL) {
                    LinearProgressIndicator(
                        progress = { minOf(shown.done, shown.total).toFloat() / shown.total.toFloat() },
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                }
            }
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
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ClosingScreenPreview() {
    BaeTheme {
        ClosingScreen(steps = LatestStream.closingSteps(BridgeClosingStep.STOPPING_SYNC).only)
    }
}

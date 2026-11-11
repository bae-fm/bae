package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidProgress
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.stepFraction
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeLoadStep
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.bridgeLoadStepKey
import uniffi.bae_bridge.bridgeOpenStepKey

/**
 * The counted step a restore, join or reset runs, opening or loading, over a
 * bar of how much of it is done, drawn from [steps] each display frame; [idle]
 * while it runs none. A step starting or ending tells [onStepping].
 * [loadStepKey] and [openStepKey] give a step's catalog key.
 */
@Composable
internal fun BootstrapStepLine(
    steps: LatestSource<BridgeBootstrapStep>?,
    modifier: Modifier = Modifier,
    onStepping: (Boolean) -> Unit = {},
    loadStepKey: (BridgeLoadStep) -> String = ::bridgeLoadStepKey,
    openStepKey: (BridgeOpenStep) -> String = ::bridgeOpenStepKey,
    idle: @Composable () -> Unit,
) {
    val context = LocalContext.current
    AndroidProgress(
        steps = steps,
        title = { step ->
            when (step) {
                is BridgeBootstrapStep.Opening -> context.coreString(openStepKey(step.progress.step))
                is BridgeBootstrapStep.Loading -> context.coreString(loadStepKey(step.progress.step))
            }
        },
        fraction = { step ->
            when (step) {
                is BridgeBootstrapStep.Opening -> stepFraction(step.progress.done, step.progress.total)
                is BridgeBootstrapStep.Loading -> stepFraction(step.progress.done, step.progress.total)
            }?.toDouble()
        },
        modifier = modifier,
        onStepping = onStepping,
        idle = idle,
    )
}

/**
 * What a running restore, join or reset waits on now, a line each; none
 * while it runs a counted step, which its [BootstrapStepLine] draws, while it
 * waits for approval, or before it says where it stands, where the caller
 * names the call itself.
 */
@Composable
internal fun StatusLines(status: BridgeBootstrapStatus?) {
    val context = LocalContext.current
    val lines =
        when (status) {
            null, BridgeBootstrapStatus.Opening, BridgeBootstrapStatus.Loading -> {
                emptyList()
            }

            BridgeBootstrapStatus.Connecting -> {
                listOf(stringResource(R.string.onboarding_status_connecting))
            }

            BridgeBootstrapStatus.Registering -> {
                listOf(stringResource(R.string.onboarding_status_registering))
            }

            BridgeBootstrapStatus.Committing -> {
                listOf(stringResource(R.string.onboarding_status_finishing))
            }

            is BridgeBootstrapStatus.Pending -> {
                status.records.map { context.coreString(it.reason.key) }
            }

            is BridgeBootstrapStatus.WaitingForApproval -> {
                emptyList()
            }
        }
    lines.forEach { line ->
        Text(
            text = line,
            style = ThemeText.body.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * A device reset loading its library: the counted step it is on over its bar,
 * drawn from [steps] each display frame, or a spinner over what it waits on,
 * which before its first report is the reset itself.
 */
@Composable
internal fun ResettingScreen(
    status: BridgeBootstrapStatus?,
    steps: LatestSource<BridgeBootstrapStep>?,
) {
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            // A counted step's own bar stands in for the spinner.
            var counting by remember(steps) { mutableStateOf(false) }
            if (!counting) {
                AndroidSpinner()
                Spacer(modifier = Modifier.height(ThemeSpace.related))
            }
            BootstrapStepLine(
                steps = steps,
                modifier = Modifier.widthIn(max = STEP_LINE_MAX_WIDTH),
                onStepping = { counting = it },
            ) {
                if (status == null) {
                    Text(
                        text = stringResource(R.string.loading_resetting_device),
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                } else {
                    StatusLines(status)
                }
            }
        }
    }
}

/** How wide a restore's, join's or reset's step line is. */
internal val STEP_LINE_MAX_WIDTH = 360.dp

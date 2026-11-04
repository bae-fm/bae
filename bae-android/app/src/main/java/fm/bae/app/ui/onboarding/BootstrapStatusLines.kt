package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.StepBar
import fm.bae.app.ui.components.stepFraction
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeLoadStep
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.bridgeLoadStepKey
import uniffi.bae_bridge.bridgeOpenStepKey

/** Whether [this] status draws its step over a bar of its own. */
internal fun BridgeBootstrapStatus?.hasStepBar(): Boolean =
    when (this) {
        is BridgeBootstrapStatus.Opening, is BridgeBootstrapStatus.Loading -> true
        else -> false
    }

/**
 * What a running restore, join or reset waits on now, a line each, or the
 * step it is on over a bar of how much of it is done; none while it waits for
 * approval, or before it says where it stands, where the caller names the
 * call itself.
 */
@Composable
internal fun StatusLines(
    status: BridgeBootstrapStatus?,
    loadStepKey: (BridgeLoadStep) -> String = ::bridgeLoadStepKey,
    openStepKey: (BridgeOpenStep) -> String = ::bridgeOpenStepKey,
) {
    val context = LocalContext.current
    val lines =
        when (status) {
            null -> {
                emptyList()
            }

            BridgeBootstrapStatus.Connecting -> {
                listOf(stringResource(R.string.onboarding_status_connecting))
            }

            is BridgeBootstrapStatus.Opening -> {
                StepBar(
                    title = context.coreString(openStepKey(status.progress.step)),
                    fraction = stepFraction(status.progress.done, status.progress.total),
                )
                emptyList()
            }

            is BridgeBootstrapStatus.Loading -> {
                StepBar(
                    title = context.coreString(loadStepKey(status.progress.step)),
                    fraction = stepFraction(status.progress.done, status.progress.total),
                )
                emptyList()
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
 * A device reset loading its library: the step it is on over its bar, or a
 * spinner over what it waits on, which before its first report is the reset
 * itself.
 */
@Composable
internal fun ResettingScreen(status: BridgeBootstrapStatus?) {
    Box(modifier = Modifier.fillMaxSize().padding(ThemeSpace.page), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            if (!status.hasStepBar()) {
                CircularProgressIndicator()
                Spacer(modifier = Modifier.height(ThemeSpace.related))
            }
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

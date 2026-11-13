package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidMatchCode
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidStep

/**
 * This device waiting to be added from another of the person's devices: the
 * [confirmation] number to check against the one that device shows, and the
 * line it waits on. No way to cancel, since the request may be sent: Back
 * leaves the screen, which ends the pairing.
 */
@Composable
internal fun PairingWaitingStep(
    confirmation: String,
    onBack: () -> Unit,
) {
    AndroidScreen(
        title = stringResource(R.string.onboarding_add_device),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        AndroidStep(
            title = stringResource(R.string.pair_check_title),
            text = stringResource(R.string.pair_check_text),
            modifier = Modifier.weight(1f),
        ) {
            AndroidMatchCode(confirmation, waiting = stringResource(R.string.pair_waiting))
        }
    }
}

/**
 * A pairing that ended without letting this device in: one step that says so
 * and what to do. OK and Back return to the code screen.
 */
@Composable
internal fun PairingEndedStep(onBack: () -> Unit) {
    AndroidScreen(
        title = stringResource(R.string.onboarding_add_device),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        AndroidStep(
            title = stringResource(R.string.pair_ended_title),
            text = stringResource(R.string.pair_ended_text),
            top = true,
        ) {
            AndroidButton(onClick = onBack, large = true) { Text(stringResource(R.string.onboarding_ok)) }
        }
    }
}

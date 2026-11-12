package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidStep

/**
 * A join that ended without letting this device in: one step that says so and
 * whom to ask for a new invite. OK and Back return to the code screen.
 */
@Composable
internal fun JoinEndedStep(
    step: OnboardingStep.JoinEnded,
    onBack: () -> Unit,
) {
    AndroidScreen(
        title = stringResource(R.string.onboarding_join_library),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        AndroidStep(
            title = stringResource(R.string.onboarding_invite_ended_title),
            text = stringResource(R.string.onboarding_invite_ended_ask, step.invite.inviter),
            top = true,
        ) {
            AndroidButton(onClick = onBack, large = true) { Text(stringResource(R.string.onboarding_ok)) }
        }
    }
}

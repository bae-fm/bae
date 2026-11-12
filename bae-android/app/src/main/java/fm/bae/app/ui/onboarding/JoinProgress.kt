package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidMatchCode
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidStep
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeInviteInfo

/**
 * A join: "Tell <inviter> this code" over the confirmation number with the
 * line it waits on while it waits for approval, with no way to cancel, since
 * its request may be sent; in every other state the library's name over what
 * the join is doing, and Cancel while it can be.
 */
@Composable
internal fun JoinProgress(
    step: OnboardingStep.Running,
    invite: BridgeInviteInfo,
    onCancel: (() -> Unit)?,
    onBack: () -> Unit,
) {
    val status = step.state?.status
    AndroidScreen(
        title = stringResource(R.string.onboarding_join_library),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        if (status is BridgeBootstrapStatus.WaitingForApproval) {
            AndroidStep(
                title = stringResource(R.string.onboarding_tell_code, invite.inviter),
                modifier = Modifier.weight(1f),
            ) {
                AndroidMatchCode(
                    status.confirmation,
                    waiting = stringResource(R.string.onboarding_waiting_for_inviter, invite.inviter),
                )
            }
        } else {
            AndroidStep(title = invite.libraryName, modifier = Modifier.weight(1f)) {
                RunningLines(step)
                onCancel?.let { CancelButton(it) }
            }
        }
    }
}

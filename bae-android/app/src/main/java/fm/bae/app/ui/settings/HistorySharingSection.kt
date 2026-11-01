package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.PersonNameField
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.WaitingIndicator
import uniffi.bae_bridge.BridgeOperationRejection
import uniffi.bae_bridge.BridgeRequestedProfile
import uniffi.bae_bridge.BridgeRequestedProfileState
import uniffi.bae_bridge.BridgeSharing

/**
 * Who sees what this person plays: only them, or the whole household. A
 * change applies to plays from now on; earlier plays keep who could see them.
 */
@Composable
internal fun HistorySharingSection(
    session: OpenLibrary,
    sharing: BridgeSharing,
    onChanged: () -> Unit,
) {
    HistorySharingContent(sharing = sharing) { chosen ->
        session.messages.perform("set history sharing to $chosen") {
            session.appHandle.setHistorySharing(chosen)
            onChanged()
        }
    }
}

/**
 * History sharing for this device's person, once read: the choice when they
 * have a profile; without one, the profile they asked for while it is made,
 * or the form to ask for it, saying why the library refused the last one; or
 * why the person couldn't be read, with Retry.
 */
@Composable
internal fun SelfHistorySharing(
    session: OpenLibrary,
    self: SelfRead,
    requested: BridgeRequestedProfile?,
    onReloadSelf: () -> Unit,
) {
    when (self) {
        SelfRead.Loading -> {
            Unit
        }

        is SelfRead.Loaded -> {
            self.person?.let { person ->
                SectionDivider()
                when (val profile = person.profile) {
                    null -> {
                        ProfileSetup(session = session, requested = requested)
                    }

                    else -> {
                        HistorySharingSection(
                            session = session,
                            sharing = profile.historySharing,
                            onChanged = onReloadSelf,
                        )
                    }
                }
            }
        }

        is SelfRead.Failed -> {
            SectionDivider()
            Column(
                modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
                verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                Eyebrow(text = stringResource(R.string.settings_history))
                ReadFailure(error = self.error, onRetry = onReloadSelf)
            }
        }
    }
}

/** A person with no profile yet: the one they asked for while it is made, or the form that asks for it. */
@Composable
private fun ProfileSetup(
    session: OpenLibrary,
    requested: BridgeRequestedProfile?,
) {
    ProfileSetupContent(requested = requested) { name ->
        session.messages.perform("create the person's profile") {
            session.appHandle.createProfile(name)
        }
    }
}

@Composable
private fun ProfileSetupContent(
    requested: BridgeRequestedProfile?,
    onCreate: (String) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_history))
        when (val state = requested?.state) {
            is BridgeRequestedProfileState.Making -> {
                WaitingIndicator(text = stringResource(R.string.settings_profile_making))
                Text(
                    text = stringResource(R.string.settings_profile_making_detail),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            is BridgeRequestedProfileState.Refused, null -> {
                if (state is BridgeRequestedProfileState.Refused) {
                    val reason = stringResource(state.reason.labelRes())
                    Text(
                        text = stringResource(R.string.settings_profile_refused, reason),
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                Text(
                    text = stringResource(R.string.settings_profile_create_prompt),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                var name by rememberSaveable { mutableStateOf("") }
                PersonNameField(name) { name = it }
                PrimaryButton(onClick = { onCreate(name.trim()) }, enabled = name.isNotBlank()) {
                    Text(stringResource(R.string.settings_profile_create))
                }
            }
        }
    }
}

@Composable
private fun HistorySharingContent(
    sharing: BridgeSharing,
    onChoose: (BridgeSharing) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_history))
        val choices = listOf(BridgeSharing.PRIVATE, BridgeSharing.HOUSEHOLD)
        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            choices.forEachIndexed { index, choice ->
                SegmentedButton(
                    selected = sharing == choice,
                    onClick = { if (sharing != choice) onChoose(choice) },
                    shape = SegmentedButtonDefaults.itemShape(index, choices.size),
                ) {
                    Text(
                        stringResource(
                            when (choice) {
                                BridgeSharing.PRIVATE -> R.string.settings_history_private
                                BridgeSharing.HOUSEHOLD -> R.string.settings_history_household
                            },
                        ),
                    )
                }
            }
        }
        Text(
            text = stringResource(R.string.settings_history_explanation),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ProfileSetupRefusedPreview() {
    BaeTheme {
        ProfileSetupContent(
            requested =
                BridgeRequestedProfile(
                    name = "Robin",
                    state = BridgeRequestedProfileState.Refused(BridgeOperationRejection.NOT_ALLOWED),
                ),
            onCreate = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun ProfileSetupMakingPreview() {
    BaeTheme {
        ProfileSetupContent(
            requested = BridgeRequestedProfile(name = "Robin", state = BridgeRequestedProfileState.Making),
            onCreate = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun HistorySharingContentPreview() {
    BaeTheme {
        HistorySharingContent(sharing = BridgeSharing.PRIVATE, onChoose = {})
    }
}

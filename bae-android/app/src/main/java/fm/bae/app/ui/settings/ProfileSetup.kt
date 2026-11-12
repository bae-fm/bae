package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.PersonNameField
import uniffi.bae_bridge.BridgeRequestedProfile
import uniffi.bae_bridge.BridgeRequestedProfileState

/** A person with no profile yet: the one they asked for while it is made, or the form that asks for it. */
@Composable
internal fun ProfileSetup(
    requested: BridgeRequestedProfile?,
    onCreate: (String) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        when (val state = requested?.state) {
            is BridgeRequestedProfileState.Making -> {
                AndroidSpinner(text = stringResource(R.string.settings_profile_making))
                Text(
                    text = stringResource(R.string.settings_profile_making_detail),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            is BridgeRequestedProfileState.Refused, is BridgeRequestedProfileState.Stopped, null -> {
                state?.unmadeRes()?.let { why ->
                    Text(
                        text = stringResource(R.string.settings_profile_refused, stringResource(why)),
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
                AndroidButton(onClick = { onCreate(name.trim()) }, enabled = name.isNotBlank()) {
                    Text(stringResource(R.string.settings_profile_create))
                }
            }
        }
    }
}

/** Why the profile asked for wasn't made; null while it is being made. */
private fun BridgeRequestedProfileState.unmadeRes(): Int? =
    when (this) {
        BridgeRequestedProfileState.Making -> null
        is BridgeRequestedProfileState.Refused -> reason.labelRes()
        is BridgeRequestedProfileState.Stopped -> stop.labelRes()
    }

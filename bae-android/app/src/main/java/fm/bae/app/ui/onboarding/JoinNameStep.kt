package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.AndroidStep
import fm.bae.app.ui.components.NoticeLook

/**
 * The step after an invite's code reads: it asks the joining person's name,
 * which their join request carries, then joins. Back returns to the code
 * screen. The step sits above the keyboard, with the field focused. A name
 * core refuses shows as the step's [error].
 */
@Composable
internal fun JoinNameStep(
    step: OnboardingStep.JoinName,
    error: DisplayError?,
    onJoin: (String) -> Unit,
    onBack: () -> Unit,
) {
    var name by rememberSaveable { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val ready = name.isNotBlank()
    AndroidScreen(
        title = stringResource(R.string.onboarding_join_library),
        onBack = onBack,
        modifier = Modifier.fillMaxSize().imePadding(),
    ) {
        AndroidStep(
            title = stringResource(R.string.onboarding_invited_to, step.invite.libraryName),
            modifier = Modifier.verticalScroll(rememberScrollState()),
        ) {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text(stringResource(R.string.onboarding_your_name)) },
                placeholder = { Text(stringResource(R.string.onboarding_name_placeholder)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().focusRequester(focus),
                keyboardOptions =
                    KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { if (ready) onJoin(name) }),
            )
            error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
            AndroidButton(
                onClick = { onJoin(name) },
                modifier = Modifier.fillMaxWidth(),
                enabled = ready,
                large = true,
            ) {
                Text(stringResource(R.string.onboarding_join))
            }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
}

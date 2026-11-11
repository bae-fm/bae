package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.ButtonKind

private val BUTTON_GAP = 10.dp

/** Scan code and Enter code, full width, one over the other. */
@Composable
internal fun WelcomeActions(
    onScan: () -> Unit,
    onEnter: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(BUTTON_GAP),
    ) {
        AndroidButton(onClick = onScan, modifier = Modifier.fillMaxWidth(), large = true) {
            Text(stringResource(R.string.onboarding_scan_code))
        }
        AndroidButton(
            kind = ButtonKind.OUTLINED,
            onClick = onEnter,
            modifier = Modifier.fillMaxWidth(),
            large = true,
        ) {
            Text(stringResource(R.string.onboarding_enter_code))
        }
    }
}

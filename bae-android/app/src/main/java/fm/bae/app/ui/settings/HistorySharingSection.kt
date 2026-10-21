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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Eyebrow
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeSharing

private val logger = BaeLogger("bae.HistorySharingSection")

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
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    HistorySharingContent(sharing = sharing) { chosen ->
        scope.launch {
            performBridgeAction(
                logger = logger,
                operation = "set history sharing to $chosen",
                errors = LocaleErrorDisplays(context),
                showError = session.configStore::showError,
            ) {
                session.appHandle.setHistorySharing(chosen)
            }
            onChanged()
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
private fun HistorySharingContentPreview() {
    BaeTheme {
        HistorySharingContent(sharing = BridgeSharing.PRIVATE, onChoose = {})
    }
}

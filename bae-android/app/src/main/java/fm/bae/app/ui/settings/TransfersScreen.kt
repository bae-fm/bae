package fm.bae.app.ui.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.RadioRow
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.downloads.setDownloadConcurrency

/**
 * How this phone moves files: whether downloads wait for Wi-Fi, which core
 * holds with coven's setting, then how many download at once. A phone never
 * uploads release files, so there is no pause.
 */
@Composable
internal fun TransfersScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val config by session.configStore.config.collectAsState()
    val unmeteredOnly by session.downloadStore.unmeteredOnly.collectAsState()
    TransfersContent(
        unmeteredOnly = unmeteredOnly,
        concurrency = config.maxConcurrentDownloads,
        limit = config.maxConcurrentTransfers,
        onBack = onBack,
        onSetUnmeteredOnly = { unmetered ->
            session.messages.perform("set Wi-Fi only") { session.appHandle.setUnmeteredOnly(unmetered) }
        },
        onSelectConcurrency = { option -> setDownloadConcurrency(session, option) },
    )
}

/** The Transfers page over the values it shows and the changes it asks for. */
@Composable
internal fun TransfersContent(
    unmeteredOnly: LiveValue<Boolean>,
    concurrency: UInt,
    limit: UInt,
    onBack: () -> Unit,
    onSetUnmeteredOnly: (Boolean) -> Unit,
    onSelectConcurrency: (UInt) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        SettingsTopBar(title = stringResource(R.string.settings_transfers), onBack = onBack)
        Column(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            WifiOnlyRows(unmeteredOnly, onSetUnmeteredOnly)
            Footer(R.string.transfers_wifi_only_footer)
            RowCard {
                ConcurrencyRow(value = concurrency, limit = limit, onSelect = onSelectConcurrency)
            }
            Footer(R.string.transfers_concurrency_footer)
        }
    }
}

/** Download over Wi-Fi only, or why whether it is on can't be told. */
@Composable
private fun WifiOnlyRows(
    unmeteredOnly: LiveValue<Boolean>,
    onSet: (Boolean) -> Unit,
) {
    when (unmeteredOnly) {
        is LiveValue.Current -> {
            RowCard {
                CardRow(
                    title = stringResource(R.string.transfers_wifi_only),
                    trailing = { Switch(checked = unmeteredOnly.value, onCheckedChange = onSet) },
                )
            }
        }

        // Whether downloads wait for Wi-Fi is unknown while its read has failed.
        is LiveValue.Failed -> {
            Text(stringResource(R.string.transfers_wifi_only))
            ErrorDetailDisclosure(unmeteredOnly.error)
        }
    }
}

/** Simultaneous downloads, naming the count, which opens a dialog of 1 up to [limit]. */
@Composable
private fun ConcurrencyRow(
    value: UInt,
    limit: UInt,
    onSelect: (UInt) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val title = stringResource(R.string.downloads_concurrency_label)
    CardRow(title = title, subtitle = value.toString(), onClick = { open = true })
    if (open) {
        AlertDialog(
            onDismissRequest = { open = false },
            title = { Text(title) },
            text = {
                Column(modifier = Modifier.selectableGroup()) {
                    for (option in 1u..limit) {
                        RadioRow(
                            label = option.toString(),
                            checked = option == value,
                            onClick = {
                                open = false
                                onSelect(option)
                            },
                        )
                    }
                }
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = { open = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun Footer(
    @StringRes text: Int,
) {
    Text(
        text = stringResource(text),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = ThemeSpace.related),
    )
}

@Preview(showBackground = true)
@Composable
private fun TransfersContentPreview() {
    BaeTheme {
        TransfersContent(
            unmeteredOnly = LiveValue.Current(false),
            concurrency = 3u,
            limit = 8u,
            onBack = {},
            onSetUnmeteredOnly = {},
            onSelectConcurrency = {},
        )
    }
}

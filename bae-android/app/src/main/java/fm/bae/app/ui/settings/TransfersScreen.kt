package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
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
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNote
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.RowSwitch
import fm.bae.app.ui.downloads.setDownloadConcurrency

/**
 * How this phone moves files: whether pins wait for Wi-Fi, which core
 * holds with coven's setting, then how many pins run at once. A phone never
 * uploads release files, so there is no pause.
 */
@Composable
internal fun TransfersScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val config by session.configStore.config.collectAsState()
    val unmeteredOnly by session.downloadStore.unmeteredOnly.collectAsState()
    AndroidSettingsTransfers(
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
internal fun AndroidSettingsTransfers(
    unmeteredOnly: LiveValue<Boolean>,
    concurrency: UInt,
    limit: UInt,
    onBack: () -> Unit,
    onSetUnmeteredOnly: (Boolean) -> Unit,
    onSelectConcurrency: (UInt) -> Unit,
) {
    AndroidScreen(
        title = stringResource(R.string.settings_transfers),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            WifiOnlyRows(unmeteredOnly, onSetUnmeteredOnly)
            AndroidNote(stringResource(R.string.transfers_wifi_only_footer))
            AndroidCard {
                ConcurrencyRow(value = concurrency, limit = limit, onSelect = onSelectConcurrency)
            }
            AndroidNote(stringResource(R.string.transfers_concurrency_footer))
        }
    }
}

/** Pin over Wi-Fi only, or why whether it is on can't be told. */
@Composable
private fun WifiOnlyRows(
    unmeteredOnly: LiveValue<Boolean>,
    onSet: (Boolean) -> Unit,
) {
    when (unmeteredOnly) {
        is LiveValue.Current -> {
            AndroidCard {
                AndroidRow(
                    title = stringResource(R.string.transfers_wifi_only),
                    switch = RowSwitch(unmeteredOnly.value, onSet),
                )
            }
        }

        // Whether pins wait for Wi-Fi is unknown while its read has failed.
        is LiveValue.Failed -> {
            Text(stringResource(R.string.transfers_wifi_only))
            AndroidNotice(unmeteredOnly.error)
        }
    }
}

/** Pins at once, naming the count, which opens a dialog of 1 up to [limit]. */
@Composable
private fun ConcurrencyRow(
    value: UInt,
    limit: UInt,
    onSelect: (UInt) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val title = stringResource(R.string.pins_at_once)
    AndroidRow(title = title, subtitle = value.toString(), onClick = { open = true })
    if (open) {
        AndroidDialog(
            onDismissRequest = { open = false },
            title = title,
            content = {
                Column(modifier = Modifier.selectableGroup()) {
                    for (option in 1u..limit) {
                        AndroidRadio(
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
            dismissButton = {
                AndroidButton(
                    kind = ButtonKind.TEXT,
                    onClick = { open = false },
                ) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun AndroidSettingsTransfersPreview() {
    BaeTheme {
        AndroidSettingsTransfers(
            unmeteredOnly = LiveValue.Current(false),
            concurrency = 3u,
            limit = 8u,
            onBack = {},
            onSetUnmeteredOnly = {},
            onSelectConcurrency = {},
        )
    }
}

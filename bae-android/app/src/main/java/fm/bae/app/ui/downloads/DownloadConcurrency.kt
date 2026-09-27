package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val logger = BaeLogger("bae.DownloadConcurrency")

/** This device's download concurrency: how many blobs a pin fetches at once. */
@Composable
internal fun DownloadConcurrencyRow(
    session: OpenLibrary,
    value: UInt,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    DownloadConcurrencySelector(
        value = value,
        onSelect = { option -> scope.launch { setDownloadConcurrency(session, context, option) } },
    )
}

/** The concurrency picker, with [value] selected and each tap reported through [onSelect]. */
@Composable
private fun DownloadConcurrencySelector(
    value: UInt,
    onSelect: (UInt) -> Unit,
) {
    // bae-core's MAX_CONCURRENT_TRANSFERS; the bridge carries the value but not the bound.
    val options = (1u..8u).toList()
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text(
            text = stringResource(R.string.downloads_concurrency_label),
            style = ThemeText.body.style,
        )
        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            options.forEachIndexed { index, option ->
                SegmentedButton(
                    selected = option == value,
                    onClick = { onSelect(option) },
                    shape = SegmentedButtonDefaults.itemShape(index = index, count = options.size),
                ) {
                    Text(option.toString())
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadConcurrencySelectorPreview() {
    BaeTheme {
        DownloadConcurrencySelector(value = 3u, onSelect = {})
    }
}

private suspend fun setDownloadConcurrency(
    session: OpenLibrary,
    context: Context,
    value: UInt,
) {
    performBridgeAction(
        logger = logger,
        operation = "set download concurrency",
        errors = LocaleErrorLines(context),
        showError = session.configStore::showError,
    ) {
        withContext(Dispatchers.IO) {
            session.appHandle.setMaxConcurrentDownloads(value)
        }
    }
}

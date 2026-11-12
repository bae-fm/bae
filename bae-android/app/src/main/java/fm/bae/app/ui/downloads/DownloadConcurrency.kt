package fm.bae.app.ui.downloads

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.countText
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** This device's download concurrency: how many files a pin fetches at once. */
@Composable
internal fun DownloadConcurrencyRow(
    session: OpenLibrary,
    value: UInt,
    limit: UInt,
) {
    DownloadConcurrencySelector(
        value = value,
        limit = limit,
        onSelect = { option -> setDownloadConcurrency(session, option) },
    )
}

/** Sets how many files a pin fetches at once, reporting a failure to the person. */
internal fun setDownloadConcurrency(
    session: OpenLibrary,
    option: UInt,
) {
    session.messages.perform("set download concurrency") {
        withContext(Dispatchers.IO) { session.appHandle.setMaxConcurrentDownloads(option) }
    }
}

/** The concurrency picker, offering 1 up to [limit], with [value] selected and each tap reported
 *  through [onSelect]. */
@Composable
private fun DownloadConcurrencySelector(
    value: UInt,
    limit: UInt,
    onSelect: (UInt) -> Unit,
) {
    val options = (1u..limit).toList()
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
    ) {
        Text(
            text = stringResource(R.string.pins_at_once),
            style = ThemeText.body.style,
        )
        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            options.forEachIndexed { index, option ->
                SegmentedButton(
                    selected = option == value,
                    onClick = { onSelect(option) },
                    shape = SegmentedButtonDefaults.itemShape(index = index, count = options.size),
                ) {
                    Text(LocalContext.current.countText(option))
                }
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadConcurrencySelectorPreview() {
    BaeTheme {
        DownloadConcurrencySelector(value = 3u, limit = 8u, onSelect = {})
    }
}

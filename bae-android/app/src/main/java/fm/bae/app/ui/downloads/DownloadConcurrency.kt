package fm.bae.app.ui.downloads

import android.content.Context
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeException

private val logger = BaeLogger("bae.DownloadConcurrency")

/**
 * Device-local download concurrency: how many blobs a pin fetches at once (1..8).
 * Mobile has no upload control — the app makes no uploads. Writes through the
 * config setter and lets the next config snapshot re-render; a rejected value
 * surfaces an error and the prior selection stands.
 */
@Composable
internal fun DownloadConcurrencyRow(
    session: OpenLibrary,
    value: UInt,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // 1..8 = bae-core's MAX_CONCURRENT_TRANSFERS; the bridge carries the value,
    // not the bound, so the UI states the range.
    val options = (1u..8u).toList()
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        Text(
            text = stringResource(R.string.downloads_concurrency_label),
            style = MaterialTheme.typography.bodyMedium,
        )
        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
            options.forEachIndexed { index, option ->
                SegmentedButton(
                    selected = option == value,
                    onClick = { scope.launch { setDownloadConcurrency(session, context, option) } },
                    shape = SegmentedButtonDefaults.itemShape(index = index, count = options.size),
                ) {
                    Text(option.toString())
                }
            }
        }
    }
}

private suspend fun setDownloadConcurrency(
    session: OpenLibrary,
    context: Context,
    value: UInt,
) {
    try {
        withContext(Dispatchers.IO) {
            session.appHandle.setMaxConcurrentDownloads(value)
        }
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("Failed to set download concurrency", e)
        session.configStore.showError(context.localizedLine(e))
    } catch (e: Exception) {
        logger.error("Failed to set download concurrency", e)
        session.configStore.showError(e.toString())
    }
}

package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.countLabelsText
import fm.bae.app.data.DownloadPageStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.ListPageState
import fm.bae.app.data.LiveValue
import fm.bae.app.factSeparator
import fm.bae.app.queueWaitText
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidTopBar
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.library.rememberScreenRows
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.bridgePinSummaryParts

/** The Pinning screen's top bar: back, Retry for the failed pins, and Pause or Resume for the queue. */
@Composable
internal fun AndroidPinningBar(
    paused: Boolean,
    hasDownloads: Boolean,
    hasFailures: Boolean,
    onBack: () -> Unit,
    onPauseToggle: () -> Unit,
    onRetry: () -> Unit,
) {
    AndroidTopBar(onBack = onBack, title = stringResource(R.string.pinning)) {
        AndroidButton(kind = ButtonKind.TEXT, onClick = onRetry, enabled = hasFailures) {
            Text(stringResource(R.string.retry))
        }
        AndroidButton(kind = ButtonKind.TEXT, onClick = onPauseToggle, enabled = hasDownloads) {
            Text(stringResource(if (paused) R.string.resume else R.string.pause))
        }
    }
}

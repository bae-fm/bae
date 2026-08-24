package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.WindowedBrowserPageStore
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// Hold the spinner briefly past the sync trigger so it doesn't snap away before
// the refreshed rows land.
private const val PULL_REFRESH_SETTLE_MS = 900L

/**
 * The body of a library browse tab: pull-to-refresh over the four states a
 * windowed page store can be in — load failure, first-page spinner, empty
 * message, rows.
 *
 * Every one of those states renders inside a scrollable. Compose delivers the
 * pull gesture to [PullToRefreshBox] through nested scroll, and only a
 * scrollable child dispatches it, so a plain centered `Box` swallows the drag
 * and the tab refuses to refresh whenever it has no rows.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryPageContent(
    session: OpenLibrary,
    page: WindowedBrowserPageStore<*, *>,
    emptyMessage: String,
    rows: @Composable () -> Unit,
) {
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberCoroutineScope()
    val onRefresh: () -> Unit = {
        session.appHandle.triggerSync()
        refreshScope.launch {
            refreshing = true
            delay(PULL_REFRESH_SETTLE_MS)
            refreshing = false
        }
    }
    val pageError = page.error
    PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = Modifier.fillMaxSize()) {
        when {
            pageError != null && page.rows.isEmpty() -> {
                ListPlaceholder {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        Text(text = pageError.message, color = MaterialTheme.colorScheme.error)
                        TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
                    }
                }
            }

            page.loading && page.rows.isEmpty() -> {
                ListPlaceholder { CircularProgressIndicator() }
            }

            page.totalCount == 0 -> {
                ListPlaceholder {
                    Text(
                        text = emptyMessage,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            else -> {
                rows()
            }
        }
    }
}

/**
 * The full-height area a browse tab shows in place of rows: one viewport-sized
 * item in a [LazyColumn], so the pull gesture still reaches the enclosing
 * [PullToRefreshBox].
 */
@Composable
private fun ListPlaceholder(content: @Composable () -> Unit) {
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            Box(
                modifier = Modifier.fillParentMaxSize().padding(32.dp),
                contentAlignment = Alignment.Center,
            ) {
                content()
            }
        }
    }
}
